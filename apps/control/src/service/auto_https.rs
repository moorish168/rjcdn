//! Background automatic HTTPS: once a site's DNS reaches the platform, obtain
//! certificates for it.
//!
//! - `access_mode=ns`: wait until the zone's public NS set is the platform's,
//!   then DNS-01 for `zone` + `*.zone` (TXT written in the hosted zone).
//! - `access_mode=cname`: exact hostnames use HTTP-01 once they resolve to an
//!   edge; wildcards need a bound DNS provider account (GoEdge-style) so we can
//!   create `_acme-challenge` TXT on the customer's DNS via their API.
//!
//! Safety rules:
//! - Hostnames already covered by a valid certificate (uploaded or issued) are
//!   left alone, so operator-supplied certificates are never superseded.
//! - The site's `ssl_enabled` (force-HTTPS redirect) is never changed here.
//! - Failures back off (15 min doubling to 24 h) to stay clear of Let's
//!   Encrypt's failed-validation limits, and a Postgres advisory lock keeps two
//!   control-plane replicas from ordering the same certificate twice.

use crate::acme;
use crate::repository;
use crate::state::AppState;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::Notify;
use tracing::{info, warn};
use uuid::Uuid;

/// Re-check interval while DNS is not pointed at the platform yet.
const WAIT_RECHECK_SECS: i64 = 300;
/// Certificates expiring sooner than this are not considered "covering".
const MIN_REMAINING_DAYS: i64 = 30;

/// Registrable domain per the Public Suffix List (`www.example.com.cn` →
/// `example.com.cn`). `None` for IPs, bare suffixes and unknown names.
pub fn registrable_domain(hostname: &str) -> Option<String> {
    let host = hostname.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() || host.parse::<std::net::IpAddr>().is_ok() {
        return None;
    }
    psl::domain_str(&host).map(str::to_string)
}

/// Apex used for a site's hosted zone. Falls back to the hostname itself when
/// the suffix list does not know it (internal TLDs).
pub fn site_apex(hostname: &str) -> String {
    registrable_domain(hostname)
        .unwrap_or_else(|| hostname.trim().trim_end_matches('.').to_ascii_lowercase())
}

/// Zone a NS-mode site lives in: the bound zone's name, or the registrable domain.
pub async fn site_zone_name(state: &AppState, site: &repository::DomainRow) -> String {
    if let Some(zone_id) = site.dns_zone_id {
        if let Ok(Some(meta)) = repository::get_dns_zone_meta(&state.pool, zone_id).await {
            return meta.name;
        }
    }
    site_apex(&site.name)
}

/// Public NS set for a zone.
///
/// Prefer public DoH (Google / Cloudflare / AliDNS) — same view as visitors and
/// as `dig` on a normal network. The control host's `/etc/resolv.conf` stub is
/// only a fallback: on cloud images it often returns stale or empty answers and
/// used to short-circuit DoH, making the wizard say「未生效」while certificates
/// (and desktop dig) already saw the new delegation.
pub async fn lookup_ns_public(domain: &str) -> Vec<String> {
    let doh = crate::service::dns_public_check::lookup_ns_names(domain).await;
    if !doh.is_empty() {
        return doh;
    }
    crate::service::dns_query::lookup_ns(domain).await
}

pub async fn resolves_to_edge_public(state: &AppState, hostname: &str) -> bool {
    resolves_to_edge(state, hostname).await
}

/// Wake the worker now (manual retry, settings change).
pub fn kick() {
    notifier().notify_one();
}

fn notifier() -> &'static Notify {
    static NOTIFY: OnceLock<Notify> = OnceLock::new();
    NOTIFY.get_or_init(Notify::new)
}

/// Every NS name observed for the zone belongs to the platform.
pub fn ns_delegated(observed: &[String], platform: &[String]) -> bool {
    !platform.is_empty()
        && !observed.is_empty()
        && observed
            .iter()
            .all(|n| platform.iter().any(|p| p.eq_ignore_ascii_case(n)))
}

async fn resolves_to_edge(state: &AppState, hostname: &str) -> bool {
    let edges: Vec<String> = repository::list_online_nodes_with_agent(&state.pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .flat_map(|n| {
            let mut ips = Vec::new();
            if !n.public_ip.is_empty() {
                ips.push(n.public_ip);
            }
            if !n.ip.is_empty() {
                ips.push(n.ip);
            }
            ips
        })
        .collect();
    if edges.is_empty() {
        return false;
    }
    // Async resolution: the std resolver blocks a Tokio worker for the whole
    // getaddrinfo round trip.
    let lookup = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::net::lookup_host(format!("{hostname}:443")),
    )
    .await;
    match lookup {
        Ok(Ok(addrs)) => addrs
            .map(|a| a.ip().to_string())
            .any(|ip| edges.iter().any(|e| e == &ip)),
        _ => false,
    }
}

/// Hostnames among `hosts` that an active, non-expiring certificate covers.
async fn covered_hosts(state: &AppState, hosts: &[String]) -> Vec<String> {
    let mut lookup: Vec<String> = Vec::new();
    for host in hosts {
        lookup.push(host.clone());
        if let Some((_, parent)) = host.split_once('.') {
            lookup.push(parent.to_string());
            lookup.push(format!("*.{parent}"));
        }
        if let Some(apex) = registrable_domain(host) {
            lookup.push(apex);
        }
    }
    lookup.sort();
    lookup.dedup();
    let certs = repository::active_certs_named(&state.pool, &lookup)
        .await
        .unwrap_or_default();
    let cutoff = chrono::Utc::now() + chrono::Duration::days(MIN_REMAINING_DAYS);
    let usable: Vec<Vec<String>> = certs
        .iter()
        .filter(|c| c.not_after.map(|t| t > cutoff).unwrap_or(false))
        .map(|c| repository::cert_dns_names(&c.cert_pem))
        .collect();
    hosts
        .iter()
        .filter(|host| {
            usable
                .iter()
                .any(|names| names.iter().any(|n| repository::san_covers(n, host)))
        })
        .cloned()
        .collect()
}

fn contact(state: &AppState) -> Option<String> {
    let c = state.config.acme_contact_email.trim();
    (!c.is_empty()).then(|| c.to_string())
}

enum Outcome {
    /// Every hostname has a valid certificate.
    Done { issued: usize },
    /// DNS does not point at the platform yet (not a failure).
    Waiting(String),
    Failed(String),
}

async fn site_hostnames(state: &AppState, site: &repository::DomainRow) -> Vec<String> {
    let mut names: Vec<String> = repository::list_domain_hostnames(&state.pool, site.id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|h| h.hostname.trim_end_matches('.').to_ascii_lowercase())
        .collect();
    if names.is_empty() {
        names.push(site.name.trim_end_matches('.').to_ascii_lowercase());
    }
    names.sort();
    names.dedup();
    names
}

async fn process_ns_site(state: &AppState, site: &repository::DomainRow) -> Outcome {
    let Some(zone_id) = site.dns_zone_id else {
        return Outcome::Failed("NS 接入的站点没有绑定托管 Zone".into());
    };
    let zone = site_zone_name(state, site).await;
    let hosts = site_hostnames(state, site).await;
    let mut wanted = hosts.clone();
    wanted.push(zone.clone());
    if covered_hosts(state, &wanted).await.len() == wanted.len() {
        return Outcome::Done { issued: 0 };
    }
    let platform_ns = repository::platform_ns_names(&state.pool)
        .await
        .unwrap_or_default();
    let observed = lookup_ns_public(&zone).await;
    if !ns_delegated(&observed, &platform_ns) {
        return Outcome::Waiting(format!(
            "等待 {zone} 的 NS 指向平台（当前：{}）",
            if observed.is_empty() {
                "未查到".to_string()
            } else {
                observed.join(", ")
            }
        ));
    }
    match acme::issue_letsencrypt_dns01(
        state,
        &zone,
        zone_id,
        &state.config.acme_directory_url,
        contact(state).as_deref(),
    )
    .await
    {
        Ok(issued) => {
            if let Err(e) = repository::create_cert(
                &state.pool,
                Some(site.id),
                &zone,
                &issued.cert_pem,
                &issued.key_pem,
                &issued.issuer,
                true,
            )
            .await
            {
                return Outcome::Failed(format!("保存证书失败：{e}"));
            }
            let mut push_names = hosts;
            push_names.push(zone.clone());
            push_names.push(format!("*.{zone}"));
            push_names.sort();
            push_names.dedup();
            let pushed = acme::push_certificate_to_edges_names(
                state,
                &push_names,
                &issued.cert_pem,
                &issued.key_pem,
            )
            .await;
            info!(site = %site.name, zone = %zone, pushed, "auto-https DNS-01 issued");
            Outcome::Done { issued: 1 }
        }
        Err(e) => Outcome::Failed(e),
    }
}

async fn process_cname_site(state: &AppState, site: &repository::DomainRow) -> Outcome {
    let hosts = site_hostnames(state, site).await;
    let covered = covered_hosts(state, &hosts).await;
    let mut waiting = Vec::new();
    let mut issued = 0;

    // Certificate SANs follow whatever hostnames were filled in at onboarding:
    // wildcards → one DNS-01 order per base (plus apex if also listed).
    let uncovered: Vec<String> = hosts
        .iter()
        .filter(|h| !covered.contains(h))
        .cloned()
        .collect();
    let mut wildcard_bases: Vec<String> = uncovered
        .iter()
        .filter_map(|h| h.strip_prefix("*.").map(str::to_string))
        .collect();
    wildcard_bases.sort();
    wildcard_bases.dedup();

    let provider_id = repository::get_domain_dns_provider_id(&state.pool, site.id)
        .await
        .ok()
        .flatten();

    for base in &wildcard_bases {
        let mut names: Vec<String> = vec![format!("*.{base}")];
        if hosts.iter().any(|h| h == base) {
            names.insert(0, base.clone());
        }
        let still_need: Vec<String> = names
            .iter()
            .filter(|n| !covered.contains(n))
            .cloned()
            .collect();
        if still_need.is_empty() {
            continue;
        }
        let Some(provider_id) = provider_id else {
            waiting.push(format!(
                "*.{base}（CNAME 接入的泛域名需绑定 DNS 服务商账号，或改用整域托管）"
            ));
            continue;
        };
        match acme::issue_letsencrypt_dns01_provider(
            state,
            &still_need,
            provider_id,
            &state.config.acme_directory_url,
            contact(state).as_deref(),
        )
        .await
        {
            Ok(cert) => {
                let label = still_need
                    .iter()
                    .find(|n| !n.starts_with("*."))
                    .cloned()
                    .unwrap_or_else(|| still_need[0].clone());
                if let Err(e) = repository::create_cert(
                    &state.pool,
                    Some(site.id),
                    &label,
                    &cert.cert_pem,
                    &cert.key_pem,
                    &cert.issuer,
                    true,
                )
                .await
                {
                    return Outcome::Failed(format!("{label}：保存证书失败：{e}"));
                }
                let _ = acme::push_certificate_to_edges_names(
                    state,
                    &still_need,
                    &cert.cert_pem,
                    &cert.key_pem,
                )
                .await;
                issued += 1;
                info!(
                    site = %site.name,
                    names = %still_need.join(","),
                    "auto-https DNS-01 (provider) issued"
                );
            }
            Err(e) => {
                let soft = e.contains("未找到")
                    || e.contains("Zone")
                    || e.contains("传播")
                    || e.contains("timeout");
                if soft {
                    waiting.push(format!("*.{base}（{e}）"));
                } else {
                    return Outcome::Failed(format!("*.{base}：{e}"));
                }
            }
        }
    }

    for host in uncovered.iter().filter(|h| !h.starts_with("*.")) {
        // Covered by a wildcard order that also included the apex.
        if covered_hosts(state, &[host.clone()]).await.contains(host) {
            continue;
        }
        if wildcard_bases.iter().any(|b| host == b) {
            // Apex was attempted with the wildcard order above.
            continue;
        }
        if !resolves_to_edge(state, host).await {
            waiting.push(host.clone());
            continue;
        }
        match acme::issue_letsencrypt(
            state,
            host,
            &state.config.acme_directory_url,
            contact(state).as_deref(),
        )
        .await
        {
            Ok(cert) => {
                if let Err(e) = repository::create_cert(
                    &state.pool,
                    Some(site.id),
                    host,
                    &cert.cert_pem,
                    &cert.key_pem,
                    &cert.issuer,
                    true,
                )
                .await
                {
                    return Outcome::Failed(format!("{host}：保存证书失败：{e}"));
                }
                let _ =
                    acme::push_certificate_to_edges(state, host, &cert.cert_pem, &cert.key_pem)
                        .await;
                issued += 1;
                info!(site = %site.name, host = %host, "auto-https HTTP-01 issued");
            }
            Err(e) => return Outcome::Failed(format!("{host}：{e}")),
        }
    }
    if waiting.is_empty() {
        Outcome::Done { issued }
    } else {
        Outcome::Waiting(format!(
            "等待解析指向 CDN：{}",
            waiting.join(", ")
        ))
    }
}

async fn process_site(state: &AppState, site: &repository::DomainRow) {
    let outcome = if site.access_mode == "ns" {
        process_ns_site(state, site).await
    } else {
        process_cname_site(state, site).await
    };
    match outcome {
        Outcome::Done { issued } => {
            if issued > 0 {
                info!(site = %site.name, issued, "auto-https complete");
            }
            if let Err(e) = repository::auto_https_mark_active(&state.pool, site.id).await {
                warn!(site = %site.name, error = %e, "auto-https: record success failed");
            }
        }
        Outcome::Waiting(note) => {
            let _ = repository::auto_https_mark_waiting(
                &state.pool,
                site.id,
                &note,
                WAIT_RECHECK_SECS,
            )
            .await;
        }
        Outcome::Failed(error) => {
            let next = repository::auto_https_mark_failed(&state.pool, site.id, &error)
                .await
                .ok()
                .flatten();
            warn!(site = %site.name, error = %error, next_attempt = ?next, "auto-https issue failed");
        }
    }
}

/// One pass over due sites. Only one control-plane replica runs it at a time.
pub async fn tick(state: &AppState) {
    let mut conn = match state.pool.acquire().await {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "auto-https: acquire connection failed");
            return;
        }
    };
    let locked: bool =
        match sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtext('lagran-auto-https')::bigint)")
            .fetch_one(&mut *conn)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "auto-https: advisory lock failed");
                return;
            }
        };
    if !locked {
        return;
    }
    match repository::list_domains_due_for_auto_https(&state.pool).await {
        Ok(sites) => {
            for site in sites {
                process_site(state, &site).await;
            }
        }
        Err(e) => warn!(error = %e, "auto-https: list sites failed"),
    }
    let _ = sqlx::query("SELECT pg_advisory_unlock(hashtext('lagran-auto-https')::bigint)")
        .execute(&mut *conn)
        .await;
}

pub fn spawn(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick_interval = tokio::time::interval(Duration::from_secs(60));
        tick_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = tick_interval.tick() => {}
                _ = notifier().notified() => {}
            }
            tick(&state).await;
        }
    })
}

/// Queue a site for issuance by the worker. `force` (admins) skips the backoff
/// window; otherwise a site still backing off after failures is refused so a
/// user cannot spend the platform's ACME quota by clicking repeatedly.
pub async fn request_issue(
    state: &AppState,
    domain_id: Uuid,
    force: bool,
) -> Result<(), String> {
    if !force {
        if let Ok(Some(next)) = repository::auto_https_next_attempt(&state.pool, domain_id).await {
            if next > chrono::Utc::now() {
                return Err(format!(
                    "上次申请失败，为避免触发 Let's Encrypt 频率限制，请在 {} 之后重试",
                    next.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M")
                ));
            }
        }
    }
    repository::auto_https_queue(&state.pool, domain_id, false)
        .await
        .map_err(|e| e.to_string())?;
    kick();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registrable_domain_respects_public_suffixes() {
        assert_eq!(registrable_domain("www.example.com").as_deref(), Some("example.com"));
        assert_eq!(registrable_domain("a.b.example.com.cn").as_deref(), Some("example.com.cn"));
        assert_eq!(registrable_domain("shop.example.co.uk").as_deref(), Some("example.co.uk"));
        assert_eq!(registrable_domain("example.net.cn.").as_deref(), Some("example.net.cn"));
        assert_eq!(registrable_domain("com.cn"), None);
        assert_eq!(registrable_domain("203.0.113.5"), None);
    }

    #[test]
    fn site_apex_never_returns_a_bare_suffix_for_known_tlds() {
        assert_eq!(site_apex("www.example.com.cn"), "example.com.cn");
        assert_eq!(site_apex("Example.ORG"), "example.org");
    }

    #[test]
    fn ns_delegation_requires_exact_platform_names() {
        let platform = vec!["ns1.cdn.test".to_string(), "ns2.cdn.test".to_string()];
        assert!(ns_delegated(&["ns1.cdn.test".into()], &platform));
        assert!(!ns_delegated(&["ns1.other.test".into()], &platform));
        assert!(!ns_delegated(&[], &platform));
        assert!(!ns_delegated(&["ns1.cdn.test".into()], &[]));
    }
}
