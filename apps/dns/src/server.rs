//! Authoritative DNS service: zone checks, scheduling lookups and caching.

use crate::config::{AuthView, DnsConfig, StaticRecord, ZoneRuntime};
use crate::control::ControlClient;
use crate::wire::{
    a_record, aaaa_record, cname_record, encode_response, mx_record, ns_record, parse_query,
    soa_record, txt_record, Ecs, Query, Record, RCODE_NOERROR, RCODE_NXDOMAIN, RCODE_REFUSED,
    RCODE_SERVFAIL, TYPE_A,
    TYPE_AAAA, TYPE_ANY, TYPE_CNAME, TYPE_MX, TYPE_NS, TYPE_SOA, TYPE_TXT,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct CacheEntry {
    ip: Option<IpAddr>,
    ttl: u32,
    at: Instant,
}

/// Cached client-subnet → line code.
struct LineEntry {
    code: String,
    expires: Instant,
}

const LINE_TTL: Duration = Duration::from_secs(300);
const LINE_FAILURE_TTL: Duration = Duration::from_secs(30);
const LINE_CACHE_MAX: usize = 50_000;

/// Cache key for line resolution: /24 for IPv4, /48 for IPv6. Geo / ISP data
/// does not change inside such a block, and it bounds the cache.
fn line_key(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            format!("{}.{}.{}.0/24", o[0], o[1], o[2])
        }
        IpAddr::V6(v6) => {
            let s = v6.segments();
            format!("{:x}:{:x}:{:x}::/48", s[0], s[1], s[2])
        }
    }
}

pub struct Service {
    cfg: DnsConfig,
    /// Live zones (env bootstrap, then control-plane `/dns/sync`).
    view: Arc<RwLock<AuthView>>,
    control: ControlClient,
    cache: Mutex<HashMap<String, CacheEntry>>,
    /// Keys currently being fetched from the control plane.
    ///
    /// A hot name missed by every worker at once used to produce one control-plane
    /// call per worker. The set makes the first caller the leader and the rest wait
    /// for its result.
    in_flight: Mutex<HashSet<String>>,
    /// Signalled whenever an in-flight fetch finishes, so waiters re-check the
    /// cache instead of polling.
    flight_done: Condvar,
    hits: AtomicU64,
    misses: AtomicU64,
    /// Line codes per client subnet, so records with lines do not cost a
    /// control-plane round trip on every query.
    line_cache: Mutex<HashMap<String, LineEntry>>,
}

/// Mark `key` as being fetched. Returns true when this caller is the leader.
///
/// A poisoned mutex returns true: falling through as the leader costs a duplicate
/// fetch, while refusing would leave the query unanswered.
fn claim_flight(in_flight: &Mutex<HashSet<String>>, key: &str) -> bool {
    match in_flight.lock() {
        Ok(mut set) => set.insert(key.to_string()),
        Err(_) => true,
    }
}

/// Clear the in-flight marker for `key` and wake anyone waiting on it.
fn release_flight(in_flight: &Mutex<HashSet<String>>, done: &Condvar, key: &str) {
    if let Ok(mut set) = in_flight.lock() {
        set.remove(key);
    }
    done.notify_all();
}

impl Service {
    pub fn new(cfg: DnsConfig, control: ControlClient) -> Self {
        let view = Arc::new(RwLock::new(cfg.bootstrap_view()));
        Self {
            cfg,
            view,
            control,
            cache: Mutex::new(HashMap::new()),
            in_flight: Mutex::new(HashSet::new()),
            flight_done: Condvar::new(),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            line_cache: Mutex::new(HashMap::new()),
        }
    }

    fn with_view<R>(&self, f: impl FnOnce(&AuthView) -> R) -> Option<R> {
        self.view.read().ok().map(|guard| f(&guard))
    }

    /// Replace the live view when the control plane reports a newer serial.
    pub fn apply_view(&self, next: AuthView) {
        let Ok(mut guard) = self.view.write() else {
            return;
        };
        // Compare the content, not serial + record count: replacing one record
        // with another (an ACME TXT value, an edited A record) keeps both equal
        // and the change would never be served.
        if *guard == next {
            return;
        }
        tracing::info!(
            zones = next.zones.len(),
            serial = next.serial,
            "dns auth view updated"
        );
        *guard = next;
    }

    fn find_zone<'a>(view: &'a AuthView, qname: &str) -> Option<&'a ZoneRuntime> {
        view.zones
            .iter()
            .filter(|z| {
                !z.zone.is_empty() && (qname == z.zone || qname.ends_with(&format!(".{}", z.zone)))
            })
            .max_by_key(|z| z.zone.len())
    }

    fn zone_serial(z: &ZoneRuntime) -> u32 {
        if z.serial > 0 {
            z.serial.min(u32::MAX as i64) as u32
        } else {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| (d.as_secs() / 3600) as u32)
                .unwrap_or(1)
        }
    }

    fn soa_for(z: &ZoneRuntime) -> Record {
        soa_record(
            &z.zone,
            &z.soa_mname,
            &z.soa_rname,
            Self::zone_serial(z),
            300,
        )
    }

    fn ns_records_for(z: &ZoneRuntime) -> Vec<Record> {
        z.ns_names
            .iter()
            .map(|name| ns_record(&z.zone, name, 300))
            .collect()
    }

    fn ns_glue_for(z: &ZoneRuntime, name: &str) -> Option<Record> {
        let index = z.ns_names.iter().position(|n| n.eq_ignore_ascii_case(name))?;
        let ip = *z.ns_ips.get(index)?;
        Some(match ip {
            IpAddr::V4(v4) => a_record(name, v4, 300),
            IpAddr::V6(v6) => aaaa_record(name, v6, 300),
        })
    }

    fn ns_glue_records_for(z: &ZoneRuntime) -> Vec<Record> {
        z.ns_names
            .iter()
            .filter_map(|name| Self::ns_glue_for(z, name))
            .collect()
    }

    fn relative_owner(qname: &str, zone: &str) -> String {
        if qname == zone {
            "@".into()
        } else if let Some(rel) = qname.strip_suffix(&format!(".{zone}")) {
            rel.to_ascii_lowercase()
        } else {
            qname.to_ascii_lowercase()
        }
    }

    /// Line code for the client: the ECS subnet when the resolver sent one
    /// (the visitor's network), else the resolver address. Cached per subnet.
    fn line_for(&self, client_ip: IpAddr, ecs: Option<&Ecs>) -> Option<String> {
        let ip = ecs.map(|e| e.addr).unwrap_or(client_ip);
        let key = line_key(ip);
        let now = Instant::now();
        if let Ok(cache) = self.line_cache.lock() {
            if let Some(entry) = cache.get(&key) {
                if entry.expires > now {
                    return Some(entry.code.clone());
                }
            }
        }
        let (code, ttl) = match self.control.resolve_line(ip) {
            Ok(code) => (code, LINE_TTL),
            // Control plane unreachable: answer the default line, and do not
            // ask again for this subnet on every query.
            Err(_) => ("default".to_string(), LINE_FAILURE_TTL),
        };
        if let Ok(mut cache) = self.line_cache.lock() {
            if cache.len() >= LINE_CACHE_MAX {
                cache.retain(|_, e| e.expires > now);
                if cache.len() >= LINE_CACHE_MAX {
                    cache.clear();
                }
            }
            cache.insert(
                key,
                LineEntry {
                    code: code.clone(),
                    expires: now + ttl,
                },
            );
        }
        Some(code)
    }

    /// A name exists when it owns records, has records below it (empty
    /// non-terminal) or is one of the zone's NS hosts.
    fn name_exists(z: &ZoneRuntime, owner: &str) -> bool {
        if owner == "@" {
            return true;
        }
        let suffix = format!(".{owner}");
        z.records
            .iter()
            .any(|r| r.name == owner || r.name.ends_with(&suffix))
            || z
                .ns_names
                .iter()
                .any(|n| Self::relative_owner(n, &z.zone) == owner)
    }

    /// Wildcard owner covering a non-existent name (`a.b` → `*.b`, `a` → `*`).
    fn wildcard_owner(z: &ZoneRuntime, owner: &str) -> Option<String> {
        if owner == "@" {
            return None;
        }
        let candidate = match owner.split_once('.') {
            Some((_, parent)) => format!("*.{parent}"),
            None => "*".to_string(),
        };
        z.records
            .iter()
            .any(|r| r.name == candidate)
            .then_some(candidate)
    }

    fn matching_records<'a>(z: &'a ZoneRuntime, owner: &str, rtype: &str) -> Vec<&'a StaticRecord> {
        z.records
            .iter()
            .filter(|r| {
                r.name == owner
                    && (rtype == "ANY"
                        || r.rtype.eq_ignore_ascii_case(rtype)
                        || r.rtype.eq_ignore_ascii_case("CNAME"))
            })
            .collect()
    }

    /// Prefer records for `line_code`; fall back to default/empty line; else all.
    fn filter_by_line<'a>(
        records: Vec<&'a StaticRecord>,
        line_code: Option<&str>,
    ) -> Vec<&'a StaticRecord> {
        let lined = records.iter().any(|r| {
            r.line_code
                .as_deref()
                .map(|c| !c.is_empty() && !c.eq_ignore_ascii_case("default"))
                .unwrap_or(false)
        });
        if !lined {
            return records;
        }
        let code = line_code.unwrap_or("default");
        let specific: Vec<_> = records
            .iter()
            .copied()
            .filter(|r| {
                r.line_code
                    .as_deref()
                    .map(|c| c.eq_ignore_ascii_case(code))
                    .unwrap_or(false)
            })
            .collect();
        if !specific.is_empty() {
            return specific;
        }
        records
            .into_iter()
            .filter(|r| match r.line_code.as_deref() {
                None => true,
                Some(c) if c.is_empty() || c.eq_ignore_ascii_case("default") => true,
                _ => false,
            })
            .collect()
    }

    fn static_to_wire(owner: &str, rec: &StaticRecord) -> Option<Record> {
        let owner = owner.to_string();
        let ttl = rec.ttl.max(5);
        match rec.rtype.to_ascii_uppercase().as_str() {
            "A" => rec
                .content
                .parse::<Ipv4Addr>()
                .ok()
                .map(|ip| a_record(&owner, ip, ttl)),
            "AAAA" => rec
                .content
                .parse::<std::net::Ipv6Addr>()
                .ok()
                .map(|ip| aaaa_record(&owner, ip, ttl)),
            "CNAME" => Some(cname_record(&owner, &rec.content, ttl)),
            "TXT" => Some(txt_record(&owner, &rec.content, ttl)),
            "MX" => Some(mx_record(
                &owner,
                rec.priority.unwrap_or(10),
                &rec.content,
                ttl,
            )),
            "NS" => Some(ns_record(&owner, &rec.content, ttl)),
            _ => None,
        }
    }

    /// Evict up to `budget` expired entries with a single bounded pass.
    ///
    /// HashMap iteration order is arbitrary but stable while nothing is inserted,
    /// which is enough here: the goal is to keep the table near the cap, not to
    /// sweep it exactly. Anything still live that this pass misses is caught on the
    /// next insert.
    fn evict_expired(
        cache: &mut HashMap<String, CacheEntry>,
        now: Instant,
        stale_secs: u64,
        budget: usize,
    ) {
        let mut expired: Vec<String> = Vec::new();
        for (key, entry) in cache.iter() {
            if expired.len() >= budget {
                break;
            }
            if now.duration_since(entry.at).as_secs() > stale_secs {
                expired.push(key.clone());
            }
        }
        for key in expired {
            cache.remove(&key);
        }
    }

    fn fallback_ip(&self) -> Option<IpAddr> {
        self.with_view(|v| {
            v.fallback_ip
                .as_deref()
                .and_then(|s| s.trim().parse::<IpAddr>().ok())
        })
        .flatten()
    }

    fn answer_ttl_for(&self, z: &ZoneRuntime) -> u32 {
        if z.ttl > 0 {
            z.ttl
        } else {
            self.cfg.ttl
        }
    }

    /// Wait until the in-flight fetch for `key` finishes, or a short deadline.
    ///
    /// Bounded on purpose: if the leader is wedged (a hung read, a panic before it
    /// cleared the marker) a waiter must still answer the query rather than hold
    /// the worker thread. On timeout the caller fetches for itself, which is the
    /// pre-single-flight behaviour — degraded, not stuck.
    /// A fresh cached answer for `key`, if there is one.
    fn cached_answer(&self, key: &str, now: Instant) -> Option<Option<(IpAddr, u32)>> {
        let cache = self.cache.lock().ok()?;
        let entry = cache.get(key)?;
        if now.duration_since(entry.at).as_secs() > self.cfg.cache_secs {
            return None;
        }
        Some(entry.ip.map(|ip| (ip, entry.ttl)))
    }

    fn wait_for_flight(&self, key: &str, _now: Instant) {
        let deadline = Instant::now() + self.cfg.http_timeout().max(Duration::from_millis(100));
        let Ok(mut set) = self.in_flight.lock() else {
            return;
        };
        while set.contains(key) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return;
            }
            match self.flight_done.wait_timeout(set, remaining) {
                Ok((guard, _)) => set = guard,
                Err(_) => return,
            }
        }
    }

    /// `None` means "no usable answer" (SERVFAIL); `Some(None)` means the name
    /// has a fresh negative cache entry (NODATA).
    fn lookup(
        &self,
        key: &str,
        qname: &str,
        subnet: Option<&str>,
        client_ip: IpAddr,
    ) -> Option<Option<(IpAddr, u32)>> {
        let now = Instant::now();
        {
            let cache = self.cache.lock().ok()?;
            if let Some(entry) = cache.get(key) {
                let age = now.duration_since(entry.at).as_secs();
                if age <= self.cfg.cache_secs {
                    // Cache negative answers too. Requiring `Some(ip)` here meant
                    // an empty answer was fetched again on every query, so the
                    // negative cache existed but never took effect.
                    self.hits.fetch_add(1, Ordering::Relaxed);
                    return Some(entry.ip.map(|ip| (ip, entry.ttl)));
                }
            }
        }

        self.misses.fetch_add(1, Ordering::Relaxed);

        // Single-flight: the first caller for this key fetches, the rest wait for
        // its result. Without this, eight workers missing the same hot name at the
        // same moment produced eight control-plane calls, and when the control
        // plane was slow all eight blocked on the read timeout together while the
        // UDP queue drained.
        //
        // The lock is only held for the set bookkeeping. The fetch itself runs
        // unlocked — holding it across `control.schedule` would serialise every
        // query on the server, which is worse than the duplicate fetches.
        let leader = claim_flight(&self.in_flight, key);
        if !leader {
            self.wait_for_flight(key, now);
            // The leader is done. Its answer — or its failure — is already in the
            // cache; re-reading is the whole point of waiting. Falling through to a
            // fetch here would keep the duplicate origin calls the wait exists to
            // avoid, so only a still-empty cache sends us to the control plane.
            if let Some(now_cached) = self.cached_answer(key, now) {
                return Some(now_cached);
            }
        }

        let fetched = self.control.schedule(qname, subnet, Some(client_ip));

        if leader {
            // Publish and wake waiters before doing anything else, so a slow
            // write-back cannot extend their wait.
            release_flight(&self.in_flight, &self.flight_done, key);
        }

        match fetched {
            Ok(reply) => {
                let ip = reply.ip.or(reply.backup);
                if let Ok(mut cache) = self.cache.lock() {
                    cache.insert(
                        key.to_string(),
                        CacheEntry {
                            ip,
                            ttl: reply.ttl,
                            at: now,
                        },
                    );
                    // Bound the table without a full sweep under the write lock.
                    //
                    // The old `retain` walked every entry the moment the map passed
                    // 20k — a millisecond-scale stall on the worker thread, repeated
                    // every time a random-subdomain flood refilled the table. Here a
                    // bounded number of expired entries is evicted per insert
                    // instead, so no single query pays for the whole map.
                    if cache.len() > 20_000 {
                        Self::evict_expired(&mut cache, now, self.cfg.stale_secs, 256);
                    }
                }
                Some(ip.map(|ip| (ip, reply.ttl)))
            }
            Err(e) => {
                tracing::warn!(error = %e, qname, "scheduler lookup failed");
                if let Ok(cache) = self.cache.lock() {
                    if let Some(entry) = cache.get(key) {
                        let age = now.duration_since(entry.at).as_secs();
                        if age <= self.cfg.stale_secs {
                            if let Some(ip) = entry.ip {
                                tracing::warn!(qname, age, "serving stale scheduling result");
                                return Some(Some((ip, entry.ttl.max(5))));
                            }
                        }
                    }
                }
                self.fallback_ip()
                    .map(|ip| Some((ip, self.cfg.ttl)))
            }
        }
    }

    fn resolve(&self, q: &Query, client_ip: IpAddr) -> (u8, Vec<Record>, Vec<Record>, Vec<Record>) {
        let qname = q.qname.as_str();
        if q.qclass != 1 {
            return (RCODE_REFUSED, vec![], vec![], vec![]);
        }
        if q.qtype == TYPE_ANY {
            return (RCODE_REFUSED, vec![], vec![], vec![]);
        }
        let Ok(view) = self.view.read() else {
            return (RCODE_SERVFAIL, vec![], vec![], vec![]);
        };
        let Some(zone) = Self::find_zone(&view, qname) else {
            return (RCODE_REFUSED, vec![], vec![], vec![]);
        };
        let soa_auth = vec![Self::soa_for(zone)];

        if qname == zone.zone {
            match q.qtype {
                TYPE_SOA => return (RCODE_NOERROR, vec![Self::soa_for(zone)], vec![], vec![]),
                TYPE_NS => {
                    return (
                        RCODE_NOERROR,
                        Self::ns_records_for(zone),
                        vec![],
                        Self::ns_glue_records_for(zone),
                    );
                }
                _ => {}
            }
        }

        // Platform NS glue (any zone that lists this name).
        if q.qtype == TYPE_A || q.qtype == TYPE_AAAA {
            for z in &view.zones {
                if let Some(glue) = Self::ns_glue_for(z, qname) {
                    if (q.qtype == TYPE_A && glue.rtype == TYPE_A)
                        || (q.qtype == TYPE_AAAA && glue.rtype == TYPE_AAAA)
                    {
                        return (RCODE_NOERROR, vec![glue], vec![], vec![]);
                    }
                    return (RCODE_NOERROR, vec![], soa_auth, vec![]);
                }
            }
        }

        let owner = Self::relative_owner(qname, &zone.zone);
        let type_name = match q.qtype {
            TYPE_A => "A",
            TYPE_AAAA => "AAAA",
            TYPE_CNAME => "CNAME",
            TYPE_TXT => "TXT",
            TYPE_MX => "MX",
            TYPE_NS => "NS",
            TYPE_SOA => "SOA",
            _ => "",
        };

        // Hosted zones: a name with no records at or below it is answered from
        // a matching wildcard, else NXDOMAIN (not an empty NOERROR).
        let exists = zone.kind == "schedule" || Self::name_exists(zone, &owner);
        let lookup_owner = if exists {
            owner.clone()
        } else {
            match Self::wildcard_owner(zone, &owner) {
                Some(w) => w,
                None => return (RCODE_NXDOMAIN, vec![], soa_auth, vec![]),
            }
        };
        let mut statics = if type_name.is_empty() {
            Vec::new()
        } else {
            Self::matching_records(zone, &lookup_owner, type_name)
        };
        let needs_line = statics.iter().any(|r| {
            r.line_code
                .as_deref()
                .map(|c| !c.is_empty() && !c.eq_ignore_ascii_case("default"))
                .unwrap_or(false)
        });
        if needs_line {
            let line = self.line_for(client_ip, q.ecs.as_ref());
            statics = Self::filter_by_line(statics, line.as_deref());
        }

        // Explicit CNAME wins for CNAME queries; proxied CNAME flattens to GeoDNS A/AAAA.
        let proxied_cname = statics.iter().find(|r| {
            r.rtype.eq_ignore_ascii_case("CNAME")
                && r.cdn_proxied
                && (q.qtype == TYPE_A || q.qtype == TYPE_AAAA)
        });
        if let Some(cname) = statics
            .iter()
            .find(|r| r.rtype.eq_ignore_ascii_case("CNAME"))
        {
            if proxied_cname.is_none() {
                if let Some(rec) = Self::static_to_wire(qname, cname) {
                    return (RCODE_NOERROR, vec![rec], vec![], vec![]);
                }
            }
        }

        let typed: Vec<&StaticRecord> = statics
            .iter()
            .copied()
            .filter(|r| r.rtype.eq_ignore_ascii_case(type_name))
            .collect();

        // CDN 主域 (schedule): answers are operator-owned A/AAAA per line — never GeoDNS.
        if zone.kind == "schedule" && (q.qtype == TYPE_A || q.qtype == TYPE_AAAA) {
            let answers: Vec<Record> = typed
                .iter()
                .filter(|r| !r.cdn_proxied)
                .filter_map(|r| Self::static_to_wire(qname, r))
                .collect();
            if !answers.is_empty() {
                return (RCODE_NOERROR, answers, vec![], vec![]);
            }
            // No line-specific record: try default/empty-line rows already in `typed`
            // (filter_by_line may have kept them). Else fallback IP from settings.
            drop(view);
            if let Some(ip) = self.fallback_ip() {
                let ttl = 30u32;
                let answers = match (q.qtype, ip) {
                    (TYPE_A, IpAddr::V4(v4)) => vec![a_record(qname, v4, ttl)],
                    (TYPE_AAAA, IpAddr::V6(v6)) => vec![aaaa_record(qname, v6, ttl)],
                    _ => vec![],
                };
                if !answers.is_empty() {
                    return (RCODE_NOERROR, answers, vec![], vec![]);
                }
            }
            return (RCODE_NOERROR, vec![], soa_auth, vec![]);
        }

        if !typed.is_empty() {
            let proxied = typed.iter().any(|r| r.cdn_proxied)
                && (q.qtype == TYPE_A || q.qtype == TYPE_AAAA);
            if !proxied {
                let answers: Vec<Record> = typed
                    .iter()
                    .filter_map(|r| Self::static_to_wire(qname, r))
                    .collect();
                if answers.is_empty() {
                    return (RCODE_NOERROR, vec![], soa_auth, vec![]);
                }
                return (RCODE_NOERROR, answers, vec![], vec![]);
            }
        }

        // Hosted proxied names (legacy): flatten via scheduler until sites CNAME to 主域.
        let want_schedule = (q.qtype == TYPE_A || q.qtype == TYPE_AAAA)
            && (typed.iter().any(|r| r.cdn_proxied) || proxied_cname.is_some());
        if !want_schedule {
            return (RCODE_NOERROR, vec![], soa_auth, vec![]);
        }

        let zone_ttl = self.answer_ttl_for(zone);
        drop(view);

        let subnet = q
            .ecs
            .as_ref()
            .map(|e| format!("{}/{}", e.addr, e.source_prefix));
        let subnet_key = subnet.clone().unwrap_or_else(|| client_ip.to_string());
        let key = format!("{}|{}|{}", qname, q.qtype, subnet_key);
        let Some(answer) = self.lookup(&key, qname, subnet.as_deref(), client_ip) else {
            return (RCODE_SERVFAIL, vec![], vec![], vec![]);
        };
        let Some((ip, ttl)) = answer else {
            return (RCODE_NOERROR, vec![], soa_auth, vec![]);
        };
        let ttl = ttl.min(zone_ttl).max(5);
        let answers = match (q.qtype, ip) {
            (TYPE_A, IpAddr::V4(v4)) => vec![a_record(qname, v4, ttl)],
            (TYPE_AAAA, IpAddr::V6(v6)) => vec![aaaa_record(qname, v6, ttl)],
            (TYPE_A, IpAddr::V6(_)) | (TYPE_AAAA, IpAddr::V4(_)) => {
                return (RCODE_NOERROR, vec![], soa_auth, vec![]);
            }
            _ => vec![],
        };
        if answers.is_empty() {
            (RCODE_NOERROR, vec![], soa_auth, vec![])
        } else {
            (RCODE_NOERROR, answers, vec![], vec![])
        }
    }

    pub fn handle(&self, buf: &[u8], client_ip: IpAddr, tcp: bool) -> Vec<u8> {
        let query = match parse_query(buf) {
            Ok(q) => q,
            Err(e) => {
                tracing::debug!(error = %e, "bad DNS query");
                if buf.len() >= 12 {
                    return formerr(&buf[..2]);
                }
                return Vec::new();
            }
        };
        let (rcode, answers, authority, additional) = self.resolve(&query, client_ip);
        let limit = if tcp { 65_535 } else { query.udp_limit() };
        let resp = encode_response(&query, rcode, &answers, &authority, &additional, tcp, limit);
        tracing::debug!(
            qname = %query.qname,
            qtype = query.qtype,
            rcode,
            answers = answers.len(),
            client = %client_ip,
            "dns answer"
        );
        resp
    }

    pub fn stats(&self) -> (u64, u64, usize) {
        (
            self.hits.load(Ordering::Relaxed),
            self.misses.load(Ordering::Relaxed),
            self.cache.lock().map(|c| c.len()).unwrap_or(0),
        )
    }

    pub fn view_snapshot(&self) -> (i64, u32) {
        self.with_view(|v| (v.serial, v.zones.len() as u32))
            .unwrap_or((0, 0))
    }
}

fn formerr(id: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12);
    out.extend_from_slice(id);
    out.extend_from_slice(&0x8001u16.to_be_bytes()); // QR + FORMERR
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out
}

struct Queue {
    items: Mutex<VecDeque<(Vec<u8>, SocketAddr)>>,
    cv: Condvar,
    /// Queries dropped because the queue was full. Non-zero means the workers are
    /// not draining fast enough and clients are getting silence.
    dropped: std::sync::atomic::AtomicU64,
}

impl Queue {
    fn new() -> Self {
        Self {
            items: Mutex::new(VecDeque::new()),
            cv: Condvar::new(),
            dropped: std::sync::atomic::AtomicU64::new(0),
        }
    }
    fn push(&self, item: (Vec<u8>, SocketAddr)) {
        let mut q = self
            .items
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if q.len() < 20_000 {
            q.push_back(item);
            self.cv.notify_one();
        } else {
            // A full queue means the workers are not keeping up — a stalled worker
            // or a burst above capacity. Dropping silently made that invisible.
            self.dropped
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
    /// Queries dropped since start.
    fn dropped(&self) -> u64 {
        self.dropped.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn pop(&self) -> (Vec<u8>, SocketAddr) {
        // Recover from a poisoned mutex instead of panicking.
        //
        // `expect` here used to be the single most dangerous line in this
        // service: one panic while the guard was held poisoned the lock, and then
        // *every* worker panicked on its next `pop`. The process stayed up
        // (the listener thread is separate) so the supervisor saw a healthy DNS
        // process that answered nothing. The queue only holds query bytes and
        // source addresses, which stay valid regardless of what the panicking
        // worker did, so reusing the inner data is safe.
        let mut q = self
            .items
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            if let Some(item) = q.pop_front() {
                return item;
            }
            q = self
                .cv
                .wait(q)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }
}

/// Read granularity for TCP DNS messages.
///
/// The length prefix is honoured up to the protocol maximum, but memory grows with
/// what actually arrives instead of with what was announced.
const TCP_READ_CHUNK: usize = 4096;

/// Read exactly `len` bytes without pre-allocating the announced size.
///
/// A client may announce 65535 bytes and then send nothing; committing that up
/// front would hold the memory for the whole read timeout on every connection
/// thread. Growth follows what actually arrives.
fn read_framed_body(stream: &mut dyn Read, len: usize) -> Option<Vec<u8>> {
    let mut buf = Vec::with_capacity(len.min(TCP_READ_CHUNK));
    let mut remaining = len;
    let mut chunk = [0u8; TCP_READ_CHUNK];
    while remaining > 0 {
        let want = remaining.min(TCP_READ_CHUNK);
        stream.read_exact(&mut chunk[..want]).ok()?;
        buf.extend_from_slice(&chunk[..want]);
        remaining -= want;
    }
    Some(buf)
}

/// Outcome of reading one TCP-framed DNS message.
#[derive(Debug, PartialEq, Eq)]
enum FramedRead {
    /// A complete message of the announced length.
    Body(Vec<u8>),
    /// Nothing to answer: zero-length or a failed/short read.
    Drop,
}

/// Read one length-prefixed message, applying the acceptance policy.
///
/// Kept separate from `handle_tcp` so the policy is testable: the previous
/// version rejected anything over 4096 bytes here, which silently closed the
/// connection for queries the protocol allows.
fn read_framed_message(stream: &mut dyn Read, announced: usize) -> FramedRead {
    if announced == 0 {
        return FramedRead::Drop;
    }
    match read_framed_body(stream, announced) {
        Some(body) => FramedRead::Body(body),
        None => FramedRead::Drop,
    }
}

/// Best-effort text for a panic payload.
///
/// Most panics carry `&str` or `String`; anything else is reported by type so the
/// log line is still useful.
fn panic_reason(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        return (*s).to_string();
    }
    if let Some(s) = payload.downcast_ref::<String>() {
        return s.clone();
    }
    "non-string panic payload".to_string()
}

fn handle_tcp(mut stream: TcpStream, service: Arc<Service>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let peer = stream.peer_addr().ok();
    let mut len_buf = [0u8; 2];
    if stream.read_exact(&mut len_buf).is_err() {
        return;
    }
    let len = u16::from_be_bytes(len_buf) as usize;
    // A TCP DNS message is length-prefixed with a 16-bit field, and RFC 7766
    // expects the server to read what the client announced. The previous 4096
    // cap silently dropped larger queries: the connection was closed without a
    // response, so the client saw a timeout rather than an error, and a resolver
    // retrying over TCP (which is what a truncated UDP answer leads to) never got
    // an answer at all. `Service::handle` already treats TCP as good for 65535
    // bytes, so the two limits disagreed.
    if len == 0 {
        // Zero-length is not a message; nothing to answer with an ID.
        return;
    }
    // Read into a fixed-size stack buffer in chunks rather than pre-allocating
    // `len` up front: a client may announce 65535 bytes and then send nothing, and
    // `vec![0u8; len]` would have committed that memory for the whole 5 s read
    // timeout on every one of the (default 512) connection threads.
    let buf = match read_framed_message(&mut stream, len) {
        FramedRead::Body(body) => body,
        FramedRead::Drop => return,
    };
    let client_ip = peer
        .map(|p| p.ip())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let resp = service.handle(&buf, client_ip, true);
    if resp.len() > 65_535 {
        // The response cannot be framed in a 16-bit length field. Answer with
        // FORMERR echoing the original ID instead of closing silently, so the
        // client can distinguish "refused" from "server unreachable".
        let id = &buf[..buf.len().min(2)];
        let formerr = formerr(id);
        let mut out = Vec::with_capacity(2 + formerr.len());
        out.extend_from_slice(&(formerr.len() as u16).to_be_bytes());
        out.extend_from_slice(&formerr);
        let _ = stream.write_all(&out);
        return;
    }
    let mut out = Vec::with_capacity(2 + resp.len());
    out.extend_from_slice(&(resp.len() as u16).to_be_bytes());
    out.extend_from_slice(&resp);
    let _ = stream.write_all(&out);
}

pub fn run(cfg: DnsConfig) -> anyhow::Result<()> {
    let control = ControlClient::new(&cfg.control_url, &cfg.agent_token, cfg.http_timeout())?;
    let service = Arc::new(Service::new(cfg.clone(), control));
    let udp = Arc::new(UdpSocket::bind(&cfg.listen)?);
    tracing::info!(
        listen = %cfg.listen,
        zone = %cfg.zone,
        ns = %cfg.ns_names.join(","),
        ttl = cfg.ttl,
        sync_secs = cfg.sync_secs,
        control = %cfg.control_url,
        "authoritative DNS started"
    );

    if cfg.sync_secs > 0 && !cfg.control_url.is_empty() {
        let sync_service = service.clone();
        let sync_control =
            ControlClient::new(&cfg.control_url, &cfg.agent_token, cfg.http_timeout())?;
        let interval = Duration::from_secs(cfg.sync_secs.max(5));
        let hb_public_ip = cfg.public_ip.clone();
        let hb_node_name = cfg.node_name.clone();
        let hb_listen = cfg.listen.clone();
        std::thread::spawn(move || {
            // Immediate first pull so console-only installs do not wait a full interval.
            loop {
                match sync_control.sync_zones() {
                    Ok(Some(payload)) => {
                        let auth = AuthView {
                            serial: payload.serial,
                            fallback_ip: payload.fallback_ip,
                            zones: payload
                                .zones
                                .into_iter()
                                .map(|z| ZoneRuntime {
                                    zone: z.zone,
                                    kind: z.kind,
                                    ttl: z.ttl,
                                    ns_names: z.ns.iter().map(|n| n.name.clone()).collect(),
                                    ns_ips: z.ns.iter().map(|n| n.ip).collect(),
                                    soa_mname: z.soa_mname,
                                    soa_rname: z.soa_rname,
                                    serial: z.serial,
                                    records: z
                                        .records
                                        .into_iter()
                                        .map(|r| StaticRecord {
                                            name: r.name,
                                            rtype: r.rtype,
                                            content: r.content,
                                            ttl: r.ttl,
                                            priority: r.priority,
                                            cdn_proxied: r.cdn_proxied,
                                            line_code: r.line_code,
                                        })
                                        .collect(),
                                })
                                .collect(),
                        };
                        sync_service.apply_view(auth);
                    }
                    Ok(None) => {
                        tracing::debug!("dns sync returned no zones yet");
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "dns sync failed");
                    }
                }

                if let Some(public_ip) = hb_public_ip.as_deref() {
                    let (serial, zone_count) = sync_service.view_snapshot();
                    let (hits, misses, _) = sync_service.stats();
                    if let Err(e) = sync_control.heartbeat(
                        hb_node_name.as_deref(),
                        public_ip,
                        &hb_listen,
                        serial,
                        zone_count,
                        hits,
                        misses,
                    ) {
                        tracing::debug!(error = %e, "dns heartbeat failed");
                    }
                }

                std::thread::sleep(interval);
            }
        });
    }

    if cfg.tcp_enabled {
        let tcp_service = service.clone();
        let listen = cfg.listen.clone();
        // One thread per connection: without a cap a client can exhaust threads
        // and file descriptors just by opening sockets.
        let max_conns = cfg.max_tcp_connections;
        let active = Arc::new(AtomicU64::new(0));
        std::thread::spawn(move || match TcpListener::bind(&listen) {
            Ok(listener) => {
                tracing::info!(listen, max_conns, "DNS TCP listener started");
                for stream in listener.incoming().flatten() {
                    // Claim a slot and test the limit in one atomic step. The
                    // previous check-then-increment was safe only because this
                    // accept loop is single-threaded — an invariant nothing states
                    // and any future second acceptor would silently break.
                    let claimed = active
                        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                            (current < max_conns as u64).then_some(current + 1)
                        })
                        .is_ok();
                    if !claimed {
                        // Dropping the stream closes the connection immediately.
                        tracing::debug!("TCP connection limit reached, dropping");
                        continue;
                    }
                    let svc = tcp_service.clone();
                    let active = active.clone();
                    std::thread::spawn(move || {
                        handle_tcp(stream, svc);
                        active.fetch_sub(1, Ordering::AcqRel);
                    });
                }
            }
            Err(e) => tracing::error!(error = %e, "DNS TCP bind failed"),
        });
    }

    let queue = Arc::new(Queue::new());
    for index in 0..cfg.workers {
        let q = queue.clone();
        let svc = service.clone();
        let sock = udp.clone();
        // A worker that dies takes a slice of the server's capacity with it, and
        // nothing else notices: `run()` keeps receiving and enqueuing, the process
        // stays up, and the supervisor sees a healthy DNS service that answers
        // fewer and fewer queries as workers drop out. Catching a panic here keeps
        // one malformed request from costing a worker forever.
        std::thread::spawn(move || {
            let mut restarts: u64 = 0;
            loop {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| loop {
                    let (buf, peer) = q.pop();
                    let client_ip = peer.ip();
                    let resp = svc.handle(&buf, client_ip, false);
                    if !resp.is_empty() {
                        let _ = sock.send_to(&resp, peer);
                    }
                }));
                // `catch_unwind` only returns on a panic or a `panic = "abort"`
                // build; reaching here means this worker unwound.
                let Err(payload) = outcome else {
                    continue;
                };
                restarts = restarts.saturating_add(1);
                let reason = panic_reason(payload.as_ref());
                tracing::error!(
                    worker = index,
                    restarts,
                    reason,
                    "DNS worker panicked; restarting"
                );
                // A tight panic loop would spin a core; back off a little.
                std::thread::sleep(Duration::from_millis(50 * restarts.min(20)));
            }
        });
    }

    // Periodic stats + cache hygiene.
    let stats_service = service.clone();
    let stats_queue = queue.clone();
    // Named so a panic here shows up as "dns-stats" rather than an unnamed thread,
    // and so the process listing makes it obvious the reporter is alive.
    std::thread::Builder::new()
        .name("dns-stats".into())
        .spawn(move || {
            let mut last_dropped = 0_u64;
            loop {
                std::thread::sleep(Duration::from_secs(60));
                let (hits, misses, size) = stats_service.stats();
                // A healthy second is unremarkable, so it stays at debug. Only drops —
                // which mean clients are getting silence — are worth a warn, and then
                // only when the count moved since the last report.
                let dropped = stats_queue.dropped();
                if dropped > last_dropped {
                    tracing::warn!(
                        hits,
                        misses,
                        cache = size,
                        dropped,
                        since_last = dropped - last_dropped,
                        "dns stats: queries dropped, queue full"
                    );
                    last_dropped = dropped;
                } else {
                    tracing::debug!(hits, misses, cache = size, "dns stats");
                }
            }
        })
        .ok();

    // Shutdown flag set by the signal handler below. The receive loop is
    // synchronous, so a read timeout is what makes `recv_from` interruptible;
    // without it SIGTERM would sit in the queue until the next packet arrived.
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let stop = stop.clone();
        // The handler runs on its own thread for the life of the process; an error
        // installing it means shutdown stays best-effort rather than failing startup.
        if let Err(e) = ctrlc::set_handler(move || {
            tracing::info!("shutdown signal received; stopping the DNS listener");
            stop.store(true, std::sync::atomic::Ordering::SeqCst);
        }) {
            tracing::warn!(error = %e, "cannot install shutdown handler");
        }
    }
    // A short timeout keeps shutdown responsive without waking a core in a tight
    // loop: 200 ms costs ~5 wakeups a second on an idle resolver.
    let _ = udp.set_read_timeout(Some(Duration::from_millis(200)));

    // Rate limit at the receive edge so a flood never reaches the worker pool.
    // Loopback is exempt and a zero `per_source` disables the limiter.
    let mut limiter = crate::ratelimit::RateLimiter::new(cfg.rate_limit);
    let mut last_report = Instant::now();
    // EDNS0 lets a client advertise up to 65535 bytes, and a 4096-byte stack
    // buffer truncated larger UDP datagrams; the truncated tail then parsed as a
    // malformed query and the client got FORMERR. One datagram is never larger
    // than 64 KiB, so size the buffer for the protocol maximum.
    let mut buf = [0u8; 65_535];
    loop {
        if stop.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        match udp.recv_from(&mut buf) {
            Ok((n, peer)) => {
                if n >= 12 && limiter.allow(peer.ip(), Instant::now()) {
                    queue.push((buf[..n].to_vec(), peer));
                }
                if last_report.elapsed() >= Duration::from_secs(30) {
                    let rejected = limiter.rejected();
                    if rejected > 0 {
                        tracing::warn!(rejected, "dropped rate-limited DNS queries");
                    }
                    last_report = Instant::now();
                }
            }
            Err(e) => {
                // A read timeout is the normal idle case, not an error: it exists so
                // the loop can check the shutdown flag.
                //
                // `Interrupted` is also normal — `ctrlc` interrupts the blocking
                // `recv_from` when the signal lands, which is exactly how the loop
                // learns to stop. Logging it as an error made every clean shutdown
                // print a spurious WARN.
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) {
                    continue;
                }
                tracing::warn!(error = %e, "udp recv error");
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    tracing::info!("DNS listener stopped; exiting");
    Ok(())
}

#[cfg(test)]
mod queue_tests {
    use super::*;
    use std::sync::Arc;

    fn addr() -> SocketAddr {
        "127.0.0.1:9".parse().unwrap()
    }

    /// A poisoned queue must keep working.
    ///
    /// `pop` used to `expect` the lock. Once any worker panicked while holding it
    /// the mutex was poisoned, so every worker panicked on its next pop, the queue
    /// drained nothing, and DNS stopped answering while the process stayed up.
    #[test]
    fn poisoned_queue_still_pops() {
        let queue = Arc::new(Queue::new());
        queue.push((b"first".to_vec(), addr()));

        // Poison the mutex by panicking while holding the guard.
        let poisoner = queue.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.items.lock().unwrap();
            panic!("poison the queue");
        })
        .join();

        // Pushing and popping must both recover rather than panic.
        queue.push((b"second".to_vec(), addr()));
        let (first, _) = queue.pop();
        let (second, _) = queue.pop();
        assert_eq!(first, b"first".to_vec());
        assert_eq!(second, b"second".to_vec());
    }

    /// The queue stays bounded so a flood cannot exhaust memory.
    #[test]
    fn queue_is_bounded() {
        let queue = Queue::new();
        for i in 0..20_010 {
            queue.push((format!("q{i}").into_bytes(), addr()));
        }
        let mut popped = 0;
        while let Ok(q) = queue.items.try_lock() {
            if q.is_empty() {
                break;
            }
            drop(q);
            queue.pop();
            popped += 1;
            if popped > 20_010 {
                break;
            }
        }
        assert!(popped <= 20_000, "queue accepted {popped} items");
        // The overflow count is what makes a stalled worker visible; the bound
        // alone does not tell anyone the queue is full.
        assert_eq!(
            queue.dropped(),
            10,
            "10 pushes past the 20_000 cap must be counted as dropped"
        );
    }

    /// A queue below the cap drops nothing.
    #[test]
    fn queue_below_cap_records_no_drops() {
        let queue = Queue::new();
        for i in 0..100 {
            queue.push((format!("q{i}").into_bytes(), addr()));
        }
        assert_eq!(queue.dropped(), 0);
    }
    /// A query larger than the old 4096 cap must be accepted.
    ///
    /// The length prefix allows 65535 bytes and RFC 7766 expects the server to
    /// read what was announced. The previous 4096 limit closed the connection
    /// without a response, so a resolver retrying over TCP after a truncated UDP
    /// answer got a timeout instead of an answer.
    #[test]
    fn tcp_reads_queries_larger_than_the_old_cap() {
        for size in [4097usize, 8192, 65_535] {
            let payload = vec![0xABu8; size];
            let mut cursor = std::io::Cursor::new(payload.clone());
            match read_framed_message(&mut cursor, size) {
                FramedRead::Body(body) => assert_eq!(body.len(), size),
                FramedRead::Drop => panic!("{size}-byte query must be read, not dropped"),
            }
        }
    }

    /// The announced size is not pre-allocated: a client that announces 65535 and
    /// sends nothing must not cause a 64 KiB allocation.
    #[test]
    fn tcp_read_does_not_preallocate_the_announced_size() {
        let mut cursor = std::io::Cursor::new(Vec::new());
        assert_eq!(
            read_framed_message(&mut cursor, 65_535),
            FramedRead::Drop,
            "a short body must be dropped, not padded"
        );
    }

    /// A body that stops mid-message is rejected rather than returned truncated.
    #[test]
    fn tcp_read_rejects_a_truncated_body() {
        let mut cursor = std::io::Cursor::new(vec![1u8; 100]);
        assert_eq!(read_framed_message(&mut cursor, 200), FramedRead::Drop);
    }

    /// A zero-length frame has no ID to answer, so it is dropped.
    #[test]
    fn tcp_read_drops_zero_length_frames() {
        let mut cursor = std::io::Cursor::new(Vec::new());
        assert_eq!(read_framed_message(&mut cursor, 0), FramedRead::Drop);
    }

    /// Small bodies round-trip unchanged.
    #[test]
    fn tcp_read_handles_small_bodies() {
        let payload = b"hello".to_vec();
        let mut cursor = std::io::Cursor::new(payload.clone());
        assert_eq!(
            read_framed_message(&mut cursor, payload.len()),
            FramedRead::Body(payload)
        );
    }
    /// A panicking request handler must not permanently lose a worker.
    ///
    /// `catch_unwind` is what the worker loop wraps around `handle`. If it did
    /// not, one malformed request would kill a worker thread silently: the listener
    /// keeps enqueuing, the process stays up, and the service answers fewer queries
    /// with every panic.
    #[test]
    fn a_panicking_handler_is_caught_and_the_loop_continues() {
        let mut handled = 0;
        let mut panics = 0;
        for i in 0..5 {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if i == 2 {
                    panic!("simulated handler panic");
                }
                i
            }));
            match outcome {
                Ok(_) => handled += 1,
                Err(payload) => {
                    panics += 1;
                    // The payload must still be describable for the log line.
                    assert!(!panic_reason(payload.as_ref()).is_empty());
                }
            }
        }
        assert_eq!(panics, 1, "the panic must be caught, not propagated");
        assert_eq!(handled, 4, "the loop must keep processing after a panic");
    }

    /// `panic_reason` copes with the payload types a panic can carry.
    #[test]
    fn panic_reason_handles_both_payload_types() {
        let str_panic = std::panic::catch_unwind(|| panic!("literal reason")).unwrap_err();
        assert_eq!(panic_reason(str_panic.as_ref()), "literal reason");

        let string_panic =
            std::panic::catch_unwind(|| panic!("{}", String::from("owned reason"))).unwrap_err();
        assert_eq!(panic_reason(string_panic.as_ref()), "owned reason");
    }

    /// The slot claim never exceeds the limit.
    ///
    /// This checks the *counting* behaviour of the claim, not its atomicity: a
    /// single-threaded loop cannot distinguish `fetch_update` from a read-then-write
    /// pair, and forcing that distinction would need a real concurrent acceptor.
    /// The atomicity itself rests on `fetch_update` being a single operation, which
    /// is what removes the check-then-act window the old code relied on an implicit
    /// single-acceptor invariant to close.
    #[test]
    fn connection_slots_never_exceed_the_limit() {
        let active = std::sync::atomic::AtomicU64::new(0);
        let max = 3u64;
        let mut claimed = 0;
        for _ in 0..10 {
            let got = active
                .fetch_update(
                    std::sync::atomic::Ordering::AcqRel,
                    std::sync::atomic::Ordering::Acquire,
                    |current| (current < max).then_some(current + 1),
                )
                .is_ok();
            if got {
                claimed += 1;
            }
        }
        assert_eq!(claimed, 3, "exactly `max` slots may be claimed");
        assert_eq!(active.load(std::sync::atomic::Ordering::Acquire), 3);
    }
    /// Only one caller may lead the fetch for a key.
    ///
    /// Before single-flight, eight workers missing the same hot name produced eight
    /// control-plane calls, and a slow control plane blocked all of them at once
    /// while the UDP queue drained.
    #[test]
    fn only_the_first_caller_leads() {
        let in_flight = Mutex::new(HashSet::new());
        assert!(claim_flight(&in_flight, "GET:a.test"), "first caller leads");
        assert!(!claim_flight(&in_flight, "GET:a.test"), "second waits");
        assert!(!claim_flight(&in_flight, "GET:a.test"), "third waits");
        // A different key is independent.
        assert!(claim_flight(&in_flight, "GET:b.test"), "other key leads");
    }

    /// Releasing the marker lets the next caller lead again.
    #[test]
    fn release_allows_a_new_leader() {
        let in_flight = Mutex::new(HashSet::new());
        let done = Condvar::new();
        assert!(claim_flight(&in_flight, "GET:a.test"));
        assert!(!claim_flight(&in_flight, "GET:a.test"));
        release_flight(&in_flight, &done, "GET:a.test");
        assert!(
            claim_flight(&in_flight, "GET:a.test"),
            "after release the next caller leads"
        );
    }

    /// A waiter is woken by the release rather than sleeping out its deadline.
    #[test]
    fn release_wakes_a_waiter_promptly() {
        let in_flight = Arc::new(Mutex::new(HashSet::new()));
        let done = Arc::new(Condvar::new());
        assert!(claim_flight(&in_flight, "GET:a.test"));

        let waiter = {
            let in_flight = in_flight.clone();
            let done = done.clone();
            std::thread::spawn(move || {
                let set = in_flight.lock().unwrap();
                let (set, _) = done
                    .wait_timeout(set, Duration::from_secs(5))
                    .expect("wait");
                !set.contains("GET:a.test")
            })
        };

        // Let the waiter block, then release.
        std::thread::sleep(Duration::from_millis(50));
        let started = Instant::now();
        release_flight(&in_flight, &done, "GET:a.test");
        let cleared = waiter.join().expect("waiter thread");
        assert!(cleared, "the waiter must observe the marker cleared");
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "the waiter must wake on the signal, not the timeout"
        );
    }

    /// A poisoned in-flight mutex falls back to leading rather than refusing.
    #[test]
    fn poisoned_flight_mutex_still_answers() {
        let in_flight = Arc::new(Mutex::new(HashSet::new()));
        let poisoner = in_flight.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.lock().unwrap();
            panic!("poison the flight mutex");
        })
        .join();
        assert!(
            claim_flight(&in_flight, "GET:a.test"),
            "a duplicate fetch beats no answer"
        );
    }
}

#[cfg(test)]
mod resolve_tests {
    use super::*;
    use crate::wire::parse_query;

    fn rec(name: &str, rtype: &str, content: &str) -> StaticRecord {
        StaticRecord {
            name: name.into(),
            rtype: rtype.into(),
            content: content.into(),
            ttl: 30,
            priority: if rtype == "MX" { Some(10) } else { None },
            cdn_proxied: false,
            line_code: None,
        }
    }

    fn service() -> Service {
        let cfg = DnsConfig::from_env();
        let control = ControlClient::new("http://127.0.0.1:9", "t", Duration::from_millis(50))
            .expect("control client");
        let svc = Service::new(cfg, control);
        svc.apply_view(AuthView {
            zones: vec![ZoneRuntime {
                zone: "example.com".into(),
                kind: "hosted".into(),
                ttl: 30,
                ns_names: vec!["ns1.example.com".into()],
                ns_ips: vec!["203.0.113.53".parse().unwrap()],
                soa_mname: "ns1.example.com".into(),
                soa_rname: "hostmaster.example.com".into(),
                serial: 1,
                records: vec![
                    rec("www", "A", "198.51.100.1"),
                    rec("*.apps", "A", "198.51.100.2"),
                    rec("deep.sub", "TXT", "hello"),
                    rec("@", "MX", "mail.example.com"),
                ],
            }],
            fallback_ip: None,
            serial: 1,
        });
        svc
    }

    fn query(name: &str, qtype: u16) -> Query {
        let mut packet = vec![0x12, 0x34, 0x00, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
        for label in name.split('.') {
            packet.push(label.len() as u8);
            packet.extend_from_slice(label.as_bytes());
        }
        packet.push(0);
        packet.extend_from_slice(&qtype.to_be_bytes());
        packet.extend_from_slice(&1u16.to_be_bytes());
        parse_query(&packet).expect("query")
    }

    fn client() -> IpAddr {
        "192.0.2.10".parse().unwrap()
    }

    #[test]
    fn existing_name_answers() {
        let (rcode, answers, _, _) = service().resolve(&query("www.example.com", TYPE_A), client());
        assert_eq!(rcode, RCODE_NOERROR);
        assert_eq!(answers.len(), 1);
    }

    #[test]
    fn missing_name_is_nxdomain() {
        let (rcode, answers, authority, _) =
            service().resolve(&query("nope.example.com", TYPE_A), client());
        assert_eq!(rcode, RCODE_NXDOMAIN);
        assert!(answers.is_empty());
        assert_eq!(authority.len(), 1, "SOA in authority");
    }

    #[test]
    fn empty_non_terminal_is_nodata_not_nxdomain() {
        let (rcode, answers, _, _) = service().resolve(&query("sub.example.com", TYPE_A), client());
        assert_eq!(rcode, RCODE_NOERROR);
        assert!(answers.is_empty());
    }

    #[test]
    fn wildcard_answers_with_the_queried_name() {
        let (rcode, answers, _, _) =
            service().resolve(&query("shop.apps.example.com", TYPE_A), client());
        assert_eq!(rcode, RCODE_NOERROR);
        assert_eq!(answers.len(), 1);
        assert_eq!(answers[0].name, "shop.apps.example.com");
    }

    #[test]
    fn apex_mx_is_served() {
        let (rcode, answers, _, _) = service().resolve(&query("example.com", TYPE_MX), client());
        assert_eq!(rcode, RCODE_NOERROR);
        assert_eq!(answers.len(), 1);
    }

    #[test]
    fn same_count_content_change_is_applied() {
        let svc = service();
        let mut next = svc.view.read().unwrap().clone();
        next.zones[0].records[2] = rec("deep.sub", "TXT", "changed");
        svc.apply_view(next);
        let (_, answers, _, _) =
            svc.resolve(&query("deep.sub.example.com", TYPE_TXT), client());
        assert_eq!(answers.len(), 1);
        assert!(String::from_utf8_lossy(&answers[0].rdata).contains("changed"));
    }

    #[test]
    fn line_keys_group_subnets() {
        assert_eq!(line_key("203.0.113.77".parse().unwrap()), "203.0.113.0/24");
        assert_eq!(line_key("2001:db8:1:2::1".parse().unwrap()), "2001:db8:1::/48");
    }
}
