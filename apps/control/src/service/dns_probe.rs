//! Active health checks for registered authoritative DNS nodes.
//!
//! Operators manage NS/glue in the console; this worker probes each node's
//! public IP on :53 so online/offline does not depend on dns.env heartbeat.

use sqlx::PgPool;
use std::net::SocketAddr;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;

fn encode_name(out: &mut Vec<u8>, name: &str) {
    for label in name.trim_end_matches('.').split('.') {
        if label.is_empty() {
            continue;
        }
        let bytes = label.as_bytes();
        let len = bytes.len().min(63);
        out.push(len as u8);
        out.extend_from_slice(&bytes[..len]);
    }
    out.push(0);
}

fn build_soa_query(zone: &str) -> Vec<u8> {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u16)
        .unwrap_or(1);
    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // RD=0
    out.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    encode_name(&mut out, zone);
    out.extend_from_slice(&6u16.to_be_bytes()); // SOA
    out.extend_from_slice(&1u16.to_be_bytes()); // IN
    out
}

async fn probe_udp(addr: SocketAddr, zone: &str) -> bool {
    let Ok(sock) = UdpSocket::bind("0.0.0.0:0").await else {
        return false;
    };
    let query = build_soa_query(zone);
    if sock.send_to(&query, addr).await.is_err() {
        return false;
    }
    let mut buf = [0u8; 512];
    match tokio::time::timeout(Duration::from_millis(1200), sock.recv_from(&mut buf)).await {
        Ok(Ok((n, _))) if n >= 12 => {
            // QR bit set and not SERVFAIL/REFUSED is good enough for liveness.
            let flags = u16::from_be_bytes([buf[2], buf[3]]);
            let rcode = (flags & 0x000F) as u8;
            (flags & 0x8000) != 0 && rcode != 2 && rcode != 5
        }
        _ => false,
    }
}

/// Keep `dns_nodes` rows aligned with schedule-zone NS/glue entries.
///
/// A node is a server, keyed by IP: several NS names on one IP are the same
/// machine, and the node list shows all of them on its row. The node is named
/// after the first of those names. Upserting every pair used to rename it to
/// the last one, so with ns1 and ns2 on one IP only ns2 seemed to exist.
pub async fn upsert_from_ns(pool: &PgPool, ns: &[(String, String)]) -> Result<(), sqlx::Error> {
    let mut seen_ips = std::collections::HashSet::new();
    for (name, ip) in ns {
        if !seen_ips.insert(ip.as_str()) {
            continue;
        }
        let res = sqlx::query(
            "INSERT INTO dns_nodes (name, public_ip, listen, status, enabled, notes)
             VALUES ($1, $2, '0.0.0.0:53', 'pending', TRUE, 'from schedule NS')
             ON CONFLICT (public_ip) DO UPDATE
             SET name = EXCLUDED.name,
                 notes = CASE
                     WHEN dns_nodes.notes = '' OR dns_nodes.notes = 'from schedule NS'
                     THEN EXCLUDED.notes
                     ELSE dns_nodes.notes
                 END,
                 updated_at = NOW()",
        )
        .bind(name)
        .bind(ip)
        .execute(pool)
        .await;

        if let Err(e) = res {
            // Name may collide with another IP; fall back to a unique name.
            if e.as_database_error()
                .map(|d| d.is_unique_violation())
                .unwrap_or(false)
            {
                let fallback = format!("dns-{ip}");
                sqlx::query(
                    "INSERT INTO dns_nodes (name, public_ip, listen, status, enabled, notes)
                     VALUES ($1, $2, '0.0.0.0:53', 'pending', TRUE, 'from schedule NS')
                     ON CONFLICT (public_ip) DO UPDATE SET updated_at = NOW()",
                )
                .bind(&fallback)
                .bind(ip)
                .execute(pool)
                .await?;
            } else {
                return Err(e);
            }
        }
    }
    Ok(())
}

pub fn spawn(pool: PgPool, interval_secs: u64) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let interval = Duration::from_secs(interval_secs.clamp(15, 600));
        tracing::info!(?interval, "dns node probe started");
        let mut tick = tokio::time::interval(interval);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            if let Err(e) = probe_once(&pool).await {
                tracing::warn!(error = %e, "dns node probe failed");
            }
        }
    })
}

async fn probe_once(pool: &PgPool) -> Result<(), sqlx::Error> {
    let zone: Option<String> = sqlx::query_scalar(
        "SELECT z.name
         FROM dns_settings s
         JOIN dns_zones z ON z.id = s.schedule_zone_id
         WHERE z.enabled = TRUE",
    )
    .fetch_optional(pool)
    .await?;
    let Some(zone) = zone.filter(|z| !z.is_empty()) else {
        return Ok(());
    };

    let nodes: Vec<(uuid::Uuid, String)> = sqlx::query_as(
        "SELECT id, public_ip FROM dns_nodes
         WHERE enabled = TRUE AND status <> 'disabled'",
    )
    .fetch_all(pool)
    .await?;

    for (id, ip) in nodes {
        let addr = match format!("{ip}:53").parse::<SocketAddr>() {
            Ok(a) => a,
            Err(_) => continue,
        };
        let started = Instant::now();
        let ok = probe_udp(addr, &zone).await;
        let elapsed = started.elapsed().as_millis() as i64;
        if ok {
            sqlx::query(
                "UPDATE dns_nodes
                 SET status = 'online',
                     last_seen_at = NOW(),
                     updated_at = NOW()
                 WHERE id = $1",
            )
            .bind(id)
            .execute(pool)
            .await?;
            tracing::debug!(%ip, elapsed_ms = elapsed, "dns node probe ok");
        } else {
            sqlx::query(
                "UPDATE dns_nodes
                 SET status = 'offline',
                     updated_at = NOW()
                 WHERE id = $1",
            )
            .bind(id)
            .execute(pool)
            .await?;
            tracing::debug!(%ip, elapsed_ms = elapsed, "dns node probe failed");
        }
    }
    Ok(())
}
