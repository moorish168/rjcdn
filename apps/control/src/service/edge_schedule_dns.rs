//! Keep CDN 主域 (schedule zone) A/AAAA rows in sync with edge nodes.
//!
//! Online + public_ip → enabled **static** (non-proxied) A/AAAA at `@` for the
//! node's line (or the default line). DNS answers these by line — not GeoDNS.
//! Offline / disabled / empty IP → pause (`enabled = false`) or remove.
//! Delete node → CASCADE removes the row; callers bump the zone serial.

use std::net::IpAddr;

use sqlx::PgPool;
use uuid::Uuid;

use crate::repository::NodeRow;

async fn schedule_zone(pool: &PgPool) -> sqlx::Result<Option<(Uuid, i32)>> {
    sqlx::query_as(
        r#"
        SELECT z.id, z.ttl_default
        FROM dns_settings s
        JOIN dns_zones z ON z.id = s.schedule_zone_id
        WHERE z.kind = 'schedule' AND z.enabled = TRUE
        "#,
    )
    .fetch_optional(pool)
    .await
}

async fn bump_serial(pool: &PgPool, zone_id: Uuid) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE dns_zones SET serial = serial + 1, updated_at = NOW() WHERE id = $1",
    )
    .bind(zone_id)
    .execute(pool)
    .await?;
    Ok(())
}

fn record_type_for_ip(ip: &str) -> Option<&'static str> {
    match ip.parse::<IpAddr>() {
        Ok(IpAddr::V4(_)) => Some("A"),
        Ok(IpAddr::V6(_)) => Some("AAAA"),
        Err(_) => None,
    }
}

/// Align one node's schedule-zone row with its current status / public IP.
pub async fn sync_node(pool: &PgPool, node: &NodeRow) -> sqlx::Result<bool> {
    let Some((zone_id, ttl_default)) = schedule_zone(pool).await? else {
        return Ok(false);
    };
    let ttl = ttl_default.clamp(5, 300);
    let ip = node.public_ip.trim();
    let rtype = record_type_for_ip(ip);
    let want_enabled = node.status == "online" && rtype.is_some();

    let existing: Option<(Uuid, String, String, bool, bool, Option<Uuid>)> = sqlx::query_as(
        r#"
        SELECT id, rtype, content, enabled, cdn_proxied, line_id
        FROM dns_records
        WHERE managed_by_node_id = $1
        LIMIT 1
        "#,
    )
    .bind(node.id)
    .fetch_optional(pool)
    .await?;

    // No usable public IP: drop any leftover managed row.
    if rtype.is_none() {
        if let Some((id, _, _, _, _, _)) = existing {
            sqlx::query("DELETE FROM dns_records WHERE id = $1")
                .bind(id)
                .execute(pool)
                .await?;
            bump_serial(pool, zone_id).await?;
            return Ok(true);
        }
        return Ok(false);
    }
    let rtype = rtype.unwrap();
    // NULL line_id is the catch-all「默认线路」at resolve time — same as a
    // manual record with no region. Do not materialise the default geo_regions
    // row id here, or the console shows「默认线路」vs「默认」inconsistently.
    let line_id = node.region_id;

    if let Some((id, cur_rtype, cur_content, cur_enabled, cur_proxied, cur_line)) = existing {
        if cur_rtype == rtype
            && cur_content == ip
            && cur_enabled == want_enabled
            && !cur_proxied
            && cur_line == line_id
        {
            return Ok(false);
        }
        sqlx::query(
            r#"
            UPDATE dns_records
            SET name = '@',
                rtype = $2,
                content = $3,
                ttl = $4,
                priority = NULL,
                cdn_proxied = FALSE,
                enabled = $5,
                line_id = $6,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(rtype)
        .bind(ip)
        .bind(ttl)
        .bind(want_enabled)
        .bind(line_id)
        .execute(pool)
        .await?;
        bump_serial(pool, zone_id).await?;
        return Ok(true);
    }

    // Insert; ignore rare IP collisions with a hand-made or peer row.
    let inserted = sqlx::query(
        r#"
        INSERT INTO dns_records
            (zone_id, name, rtype, content, ttl, priority, cdn_proxied, enabled, line_id, managed_by_node_id)
        VALUES ($1, '@', $2, $3, $4, NULL, FALSE, $5, $6, $7)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(zone_id)
    .bind(rtype)
    .bind(ip)
    .bind(ttl)
    .bind(want_enabled)
    .bind(line_id)
    .bind(node.id)
    .execute(pool)
    .await?;

    if inserted.rows_affected() == 0 {
        // Unique on (zone,name,rtype,content,...) collided — still try to claim
        // an orphaned row with the same content, or leave a warn for ops.
        tracing::warn!(
            node = %node.name,
            ip,
            "schedule-zone record for edge IP already exists; skip insert"
        );
        return Ok(false);
    }
    bump_serial(pool, zone_id).await?;
    Ok(true)
}

/// Sync every edge node (used after bulk offline marking / startup).
pub async fn sync_all(pool: &PgPool) -> sqlx::Result<u64> {
    let nodes = crate::repository::list_nodes(pool).await?;
    let mut n = 0u64;
    for node in &nodes {
        if sync_node(pool, node).await? {
            n += 1;
        }
    }
    Ok(n)
}

/// After a node delete (CASCADE already dropped the row), bump serial if a
/// schedule zone exists so DNS agents drop the answer from their view.
pub async fn bump_after_node_delete(pool: &PgPool) -> sqlx::Result<()> {
    if let Some((zone_id, _)) = schedule_zone(pool).await? {
        bump_serial(pool, zone_id).await?;
    }
    Ok(())
}

/// Pause managed rows for nodes that just went offline (bulk).
pub async fn pause_offline(pool: &PgPool) -> sqlx::Result<u64> {
    let Some((zone_id, _)) = schedule_zone(pool).await? else {
        return Ok(0);
    };
    let r = sqlx::query(
        r#"
        UPDATE dns_records r
        SET enabled = FALSE, updated_at = NOW()
        FROM edge_nodes n
        WHERE r.managed_by_node_id = n.id
          AND r.enabled = TRUE
          AND n.status <> 'online'
        "#,
    )
    .execute(pool)
    .await?;
    if r.rows_affected() > 0 {
        bump_serial(pool, zone_id).await?;
    }
    Ok(r.rows_affected())
}
