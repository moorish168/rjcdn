//! Authoritative DNS configuration.

use std::net::IpAddr;
use std::time::Duration;

fn normalize_control_url(raw: &str) -> String {
    let value = raw.trim().trim_end_matches('/');
    if value.is_empty() {
        "http://127.0.0.1:8080".into()
    } else {
        value.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticRecord {
    /// Relative owner (`@`, `www`, `mail`), lower-case.
    pub name: String,
    pub rtype: String,
    pub content: String,
    pub ttl: u32,
    pub priority: Option<u16>,
    pub cdn_proxied: bool,
    /// Shared geo line code; `None` means default/fallback line.
    pub line_code: Option<String>,
}

/// One authoritative zone view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneRuntime {
    /// Zone apex, lower-case, without trailing dot. Empty until configured.
    pub zone: String,
    /// `schedule` (GeoDNS default) or `hosted`.
    pub kind: String,
    pub ttl: u32,
    pub ns_names: Vec<String>,
    pub ns_ips: Vec<IpAddr>,
    pub soa_mname: String,
    pub soa_rname: String,
    pub serial: i64,
    pub records: Vec<StaticRecord>,
}

/// Full authoritative view (all zones + shared fallback).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthView {
    pub zones: Vec<ZoneRuntime>,
    pub fallback_ip: Option<String>,
    pub serial: i64,
}

#[derive(Debug, Clone)]
pub struct DnsConfig {
    pub listen: String,
    /// Bootstrap zone from env; may be empty when the console owns the zone.
    pub zone: String,
    pub ttl: u32,
    /// One or more NS names (comma separated), e.g. ns1,ns2.
    pub ns_names: Vec<String>,
    /// One A/AAAA per NS name (comma separated, same order).
    pub ns_ips: Vec<IpAddr>,
    pub soa_mname: String,
    pub soa_rname: String,
    pub control_url: String,
    pub agent_token: String,
    pub cache_secs: u64,
    pub stale_secs: u64,
    pub workers: usize,
    pub tcp_enabled: bool,
    /// Maximum concurrent TCP connections (one thread each).
    pub max_tcp_connections: usize,
    /// How often to pull `/api/v1/dns/sync` (0 disables).
    pub sync_secs: u64,
    /// Optional stable name reported in heartbeat (defaults to hostname/IP).
    pub node_name: Option<String>,
    /// Public IP registered/reported for this DNS instance.
    pub public_ip: Option<String>,
    /// Per-source query budget.
    pub rate_limit: crate::ratelimit::RateLimitConfig,
    pub fallback_ip: Option<String>,
}

impl DnsConfig {
    pub fn bootstrap_view(&self) -> AuthView {
        if self.zone.is_empty() {
            return AuthView {
                zones: Vec::new(),
                fallback_ip: self.fallback_ip.clone(),
                serial: 0,
            };
        }
        AuthView {
            zones: vec![ZoneRuntime {
                zone: self.zone.clone(),
                kind: "schedule".into(),
                ttl: self.ttl,
                ns_names: self.ns_names.clone(),
                ns_ips: self.ns_ips.clone(),
                soa_mname: self.soa_mname.clone(),
                soa_rname: self.soa_rname.clone(),
                serial: 0,
                records: Vec::new(),
            }],
            fallback_ip: self.fallback_ip.clone(),
            serial: 0,
        }
    }
}

fn split_list(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|s| s.trim().trim_end_matches('.').to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

impl DnsConfig {
    pub fn from_env() -> Self {
        // Empty is allowed: the process can start and wait for /dns/sync from the
        // control plane (console-managed schedule zone).
        let zone = std::env::var("DNS_ZONE")
            .unwrap_or_default()
            .trim()
            .trim_end_matches('.')
            .to_ascii_lowercase();

        let mut ns_names = split_list(&std::env::var("DNS_NS_NAME").unwrap_or_default());
        if ns_names.is_empty() && !zone.is_empty() {
            ns_names.push(format!("ns1.{zone}"));
        }

        let mut ns_ips: Vec<IpAddr> = std::env::var("DNS_NS_IP")
            .unwrap_or_default()
            .split(',')
            .filter_map(|s| s.trim().parse::<IpAddr>().ok())
            .collect();
        if ns_ips.is_empty() && !ns_names.is_empty() {
            ns_ips.push("127.0.0.1".parse().expect("valid fallback IP"));
        }
        // If fewer IPs than names, repeat the last one instead of dropping NS.
        while !ns_names.is_empty() && ns_ips.len() < ns_names.len() {
            let last = *ns_ips.last().expect("at least one NS IP");
            ns_ips.push(last);
        }

        let soa_mname = std::env::var("DNS_SOA_MNAME").unwrap_or_else(|_| {
            ns_names
                .first()
                .cloned()
                .unwrap_or_else(|| {
                    if zone.is_empty() {
                        String::new()
                    } else {
                        format!("ns1.{zone}")
                    }
                })
        });
        let soa_rname = std::env::var("DNS_SOA_RNAME").unwrap_or_else(|_| {
            if zone.is_empty() {
                String::new()
            } else {
                format!("hostmaster.{zone}")
            }
        });
        Self {
            // DNS_LISTEN is canonical; DNS_BIND is kept for older example files.
            listen: env_nonempty("DNS_LISTEN")
                .or_else(|| env_nonempty("DNS_BIND"))
                .unwrap_or_else(|| "0.0.0.0:53".into()),
            zone,
            ttl: std::env::var("DNS_TTL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30)
                .clamp(5, 300),
            ns_names,
            ns_ips,
            soa_mname,
            soa_rname,
            control_url: normalize_control_url(&std::env::var("CONTROL_URL").unwrap_or_default()),
            agent_token: std::env::var("AGENT_TOKEN").unwrap_or_default(),
            // Five seconds meant every hot name missed every five seconds: eight
            // workers would each call the control plane for the same qname, and if
            // it was slow they all blocked on the read timeout while the UDP queue
            // drained. Thirty seconds matches the scheduler's own answer TTL, so a
            // cached answer is no older than one the scheduler would have returned.
            cache_secs: std::env::var("DNS_CACHE_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
            stale_secs: std::env::var("DNS_STALE_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(300),
            workers: std::env::var("DNS_WORKERS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(8)
                .clamp(1, 64),
            tcp_enabled: std::env::var("DNS_TCP")
                .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
                .unwrap_or(true),
            max_tcp_connections: std::env::var("DNS_MAX_TCP_CONNECTIONS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(512)
                .clamp(16, 65_536),
            sync_secs: std::env::var("DNS_SYNC_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(15)
                .min(3_600),
            node_name: env_nonempty("DNS_NODE_NAME"),
            public_ip: env_nonempty("DNS_PUBLIC_IP").or_else(|| env_nonempty("PUBLIC_IP")),
            rate_limit: crate::ratelimit::RateLimitConfig {
                // 0 disables limiting, which is only sensible behind a scrubber.
                per_source: std::env::var("DNS_RATE_LIMIT_PER_SOURCE")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(100)
                    .min(100_000),
                per_prefix: std::env::var("DNS_RATE_LIMIT_PER_PREFIX")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1_000)
                    .min(1_000_000),
                max_tracked: std::env::var("DNS_RATE_LIMIT_MAX_TRACKED")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(50_000)
                    .clamp(1_000, 1_000_000),
            },
            fallback_ip: std::env::var("DNS_FALLBACK_IP")
                .ok()
                .filter(|s| !s.trim().is_empty()),
        }
    }

    pub fn http_timeout(&self) -> Duration {
        Duration::from_millis(
            std::env::var("DNS_CONTROL_TIMEOUT_MS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(800),
        )
    }
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_url_is_normalized() {
        assert_eq!(
            normalize_control_url("  https://control.example.com/  "),
            "https://control.example.com"
        );
        assert_eq!(normalize_control_url("   "), "http://127.0.0.1:8080");
    }
}
