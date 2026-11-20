//! Admin DNS zone / schedule-domain settings and agent sync.
//!
//! Phase 1: manage the platform schedule zone (GeoDNS apex), its NS/glue
//! pairs, and fallback IP. `lagran-dns` pulls the live view via `/dns/sync`.

use axum::{
    Json,
    extract::State,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::middleware::agent_auth::DnsAgentAuth;
use crate::middleware::authz::Actor;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: String,
}

type ApiError = (StatusCode, Json<ErrorBody>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NsEntry {
    pub name: String,
    pub ip: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DnsSettingsDto {
    pub configured: bool,
    pub zone: String,
    pub ns: Vec<NsEntry>,
    pub soa_mname: String,
    pub soa_rname: String,
    pub ttl: u32,
    pub fallback_ip: String,
    pub enabled: bool,
    pub serial: i64,
    pub zone_id: Option<Uuid>,
    /// Version a DNS node reports in its heartbeat once it has pulled the
    /// current `/dns/sync` view; the console compares the two per node.
    pub sync_serial: i64,
}

#[derive(Debug, Deserialize)]
pub struct PutDnsSettingsRequest {
    pub zone: String,
    pub ns: Vec<NsEntry>,
    #[serde(default)]
    pub soa_mname: Option<String>,
    #[serde(default)]
    pub soa_rname: Option<String>,
    #[serde(default)]
    pub ttl: Option<u32>,
    #[serde(default)]
    pub fallback_ip: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncNs {
    pub name: String,
    pub ip: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncRecord {
    pub name: String,
    pub rtype: String,
    pub content: String,
    pub ttl: u32,
    pub priority: Option<i32>,
    pub cdn_proxied: bool,
    /// Shared geo_regions.code; empty/null = 默认线路
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_code: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ResolveLineResponse {
    pub code: String,
    pub name: String,
    pub is_default: bool,
    /// ISO country from the control-plane IP library (empty = library miss / unloaded).
    pub country: String,
    /// Continent code from the IP library (NA/EU/AS/…); may be empty on legacy tables.
    pub continent: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncZone {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub soa_mname: String,
    pub soa_rname: String,
    pub ttl: u32,
    pub serial: i64,
    pub ns: Vec<SyncNs>,
    pub records: Vec<SyncRecord>,
}

#[derive(Debug, Serialize)]
pub struct DnsSyncResponse {
    /// Version of the whole view (see [`view_version`]); nodes echo it in
    /// their heartbeat. Each zone's SOA keeps its own serial.
    pub serial: i64,
    pub fallback_ip: Option<String>,
    pub schedule_zone: Option<String>,
    pub zones: Vec<SyncZone>,
}

#[derive(Debug, FromRow)]
struct ZoneRow {
    id: Uuid,
    name: String,
    kind: String,
    soa_mname: String,
    soa_rname: String,
    ttl_default: i32,
    serial: i64,
    enabled: bool,
}

#[derive(Debug, FromRow)]
struct NsRow {
    name: String,
    ip: String,
    #[allow(dead_code)]
    sort_order: i32,
}

fn bad_request(error: impl Into<String>) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorBody {
            error: error.into(),
        }),
    )
}

fn forbidden() -> ApiError {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorBody {
            error: "admin only".into(),
        }),
    )
}

fn db_error() -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorBody {
            error: "database error".into(),
        }),
    )
}

pub fn normalize_zone_name_pub(raw: &str) -> Result<String, String> {
    normalize_zone_name(raw)
}

fn normalize_zone_name(raw: &str) -> Result<String, String> {
    let name = raw
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if name.is_empty() {
        return Err("zone is required".into());
    }
    if name.len() > 253 {
        return Err("zone is too long".into());
    }
    if !name.contains('.') {
        return Err("zone must be a FQDN (e.g. edge.example.com)".into());
    }
    for label in name.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err("zone has an invalid label".into());
        }
        if !label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err("zone contains invalid characters".into());
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err("zone labels must not start or end with '-'".into());
        }
    }
    Ok(name)
}

fn normalize_ns_name(raw: &str, zone: &str) -> Result<String, String> {
    let mut name = raw
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if name.is_empty() {
        return Err("NS name is required".into());
    }
    // Allow short form `ns1` → `ns1.<zone>`.
    if !name.contains('.') {
        name = format!("{name}.{zone}");
    }
    if name.len() > 253 {
        return Err("NS name is too long".into());
    }
    Ok(name)
}

fn normalize_ip(raw: &str) -> Result<String, String> {
    let ip = raw.trim();
    if ip.is_empty() {
        return Err("NS IP is required".into());
    }
    ip.parse::<std::net::IpAddr>()
        .map(|v| v.to_string())
        .map_err(|_| format!("invalid IP address: {ip}"))
}

fn normalize_fallback(raw: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    value
        .parse::<std::net::IpAddr>()
        .map(|v| Some(v.to_string()))
        .map_err(|_| format!("invalid fallback IP: {value}"))
}

async fn load_ns(pool: &PgPool, zone_id: Uuid) -> Result<Vec<NsEntry>, sqlx::Error> {
    let rows = sqlx::query_as::<_, NsRow>(
        "SELECT name, ip, sort_order FROM dns_zone_ns
         WHERE zone_id = $1
         ORDER BY sort_order, name",
    )
    .bind(zone_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| NsEntry {
            name: r.name,
            ip: r.ip,
        })
        .collect())
}

async fn schedule_zone(pool: &PgPool) -> Result<Option<ZoneRow>, sqlx::Error> {
    // Prefer the settings pointer; fall back to any schedule-kind zone.
    let pointed: Option<ZoneRow> = sqlx::query_as(
        "SELECT z.id, z.name, z.kind, z.soa_mname, z.soa_rname, z.ttl_default, z.serial, z.enabled
         FROM dns_settings s
         JOIN dns_zones z ON z.id = s.schedule_zone_id",
    )
    .fetch_optional(pool)
    .await?;
    if pointed.is_some() {
        return Ok(pointed);
    }
    sqlx::query_as(
        "SELECT id, name, kind, soa_mname, soa_rname, ttl_default, serial, enabled
         FROM dns_zones
         WHERE kind = 'schedule'
         ORDER BY updated_at DESC
         LIMIT 1",
    )
    .fetch_optional(pool)
    .await
}

async fn fallback_ip(pool: &PgPool) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, Option<String>>("SELECT fallback_ip FROM dns_settings WHERE id = 1")
        .fetch_optional(pool)
        .await
        .map(|row| row.flatten().filter(|s| !s.trim().is_empty()))
}

/// Schedule apex used by onboarding / diagnostics (empty when unset).
pub async fn schedule_zone_name(pool: &PgPool) -> Result<Option<String>, sqlx::Error> {
    Ok(schedule_zone(pool).await?.map(|z| z.name))
}

/// Schedule zone id + name (for CNAME-mode wildcard DNS-01 TXT placement).
pub async fn schedule_zone_meta(pool: &PgPool) -> Result<Option<(Uuid, String)>, sqlx::Error> {
    Ok(schedule_zone(pool).await?.map(|z| (z.id, z.name)))
}

fn empty_settings() -> DnsSettingsDto {
    DnsSettingsDto {
        configured: false,
        zone: String::new(),
        ns: Vec::new(),
        soa_mname: String::new(),
        soa_rname: String::new(),
        ttl: 30,
        fallback_ip: String::new(),
        enabled: true,
        serial: 0,
        zone_id: None,
        sync_serial: 0,
    }
}

/// Version of everything `/dns/sync` serves: the sum of the enabled zones'
/// serials.
///
/// Every change to a zone bumps that zone's serial, so the sum moves even when
/// the edited zone is not the one with the highest serial. The previous value,
/// the maximum, stayed put in that case, and a node that had not pulled the
/// change yet looked up to date. Older lagran-dns builds report
/// `max(this, highest zone serial)`, which is this value, since a sum of
/// positive serials is never below any of them.
pub async fn view_version(pool: &PgPool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COALESCE(SUM(serial), 0)::BIGINT FROM dns_zones WHERE enabled = TRUE")
        .fetch_one(pool)
        .await
}

pub async fn get_settings(
    State(state): State<AppState>,
    actor: Actor,
) -> Result<Json<DnsSettingsDto>, ApiError> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let zone = schedule_zone(&state.pool).await.map_err(|e| {
        tracing::error!(error = %e, "load schedule zone");
        db_error()
    })?;
    let sync_serial = view_version(&state.pool).await.map_err(|e| {
        tracing::error!(error = %e, "load dns view version");
        db_error()
    })?;
    let Some(zone) = zone else {
        let fallback = fallback_ip(&state.pool).await.map_err(|e| {
            tracing::error!(error = %e, "load dns fallback");
            db_error()
        })?;
        let mut dto = empty_settings();
        dto.fallback_ip = fallback.unwrap_or_default();
        dto.sync_serial = sync_serial;
        return Ok(Json(dto));
    };
    let ns = load_ns(&state.pool, zone.id).await.map_err(|e| {
        tracing::error!(error = %e, "load dns ns");
        db_error()
    })?;
    let fallback = fallback_ip(&state.pool).await.map_err(|e| {
        tracing::error!(error = %e, "load dns fallback");
        db_error()
    })?;
    Ok(Json(DnsSettingsDto {
        configured: true,
        zone: zone.name,
        ns,
        soa_mname: zone.soa_mname,
        soa_rname: zone.soa_rname,
        ttl: zone.ttl_default.clamp(5, 300) as u32,
        fallback_ip: fallback.unwrap_or_default(),
        enabled: zone.enabled,
        serial: zone.serial,
        zone_id: Some(zone.id),
        sync_serial,
    }))
}

/// GET /api/v1/dns/settings/public-check: resolve the CDN 主域 through public
/// DNS-over-HTTPS resolvers, i.e. what visitors' resolvers actually see.
pub async fn public_check(
    State(state): State<AppState>,
    actor: Actor,
) -> Result<Json<crate::service::dns_public_check::PublicCheck>, ApiError> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let zone = schedule_zone(&state.pool).await.map_err(|e| {
        tracing::error!(error = %e, "load schedule zone");
        db_error()
    })?;
    let Some(zone) = zone else {
        return Err(bad_request("还没有配置 CDN 主域"));
    };
    if !zone.enabled {
        return Err(bad_request("CDN 主域未启用：勾选「启用 CDN 主域」并保存后再检测"));
    }
    let ns = load_ns(&state.pool, zone.id).await.map_err(|e| {
        tracing::error!(error = %e, "load dns ns");
        db_error()
    })?;
    let names: Vec<String> = ns.into_iter().map(|n| n.name).collect();
    Ok(Json(
        crate::service::dns_public_check::run(&zone.name, &names, zone.serial).await,
    ))
}

async fn replace_ns(
    tx: &mut Transaction<'_, Postgres>,
    zone_id: Uuid,
    ns: &[NsEntry],
) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM dns_zone_ns WHERE zone_id = $1")
        .bind(zone_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "clear dns ns");
            db_error()
        })?;
    for (idx, entry) in ns.iter().enumerate() {
        sqlx::query(
            "INSERT INTO dns_zone_ns (zone_id, name, ip, sort_order)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(zone_id)
        .bind(&entry.name)
        .bind(&entry.ip)
        .bind(idx as i32)
        .execute(&mut **tx)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "insert dns ns");
            db_error()
        })?;
    }
    Ok(())
}

pub async fn put_settings(
    State(state): State<AppState>,
    actor: Actor,
    Json(req): Json<PutDnsSettingsRequest>,
) -> Result<Json<DnsSettingsDto>, ApiError> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let zone_name = normalize_zone_name(&req.zone).map_err(bad_request)?;
    if req.ns.is_empty() {
        return Err(bad_request("at least one NS entry is required"));
    }
    if req.ns.len() > 13 {
        return Err(bad_request("at most 13 NS entries are allowed"));
    }
    let mut ns = Vec::with_capacity(req.ns.len());
    let mut seen = std::collections::HashSet::new();
    for entry in &req.ns {
        let name = normalize_ns_name(&entry.name, &zone_name).map_err(bad_request)?;
        let ip = normalize_ip(&entry.ip).map_err(bad_request)?;
        if !seen.insert(name.clone()) {
            return Err(bad_request(format!("duplicate NS name: {name}")));
        }
        ns.push(NsEntry { name, ip });
    }
    let soa_mname = req
        .soa_mname
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_end_matches('.').to_ascii_lowercase())
        .unwrap_or_else(|| ns[0].name.clone());
    let soa_rname = req
        .soa_rname
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_end_matches('.').to_ascii_lowercase())
        .unwrap_or_else(|| format!("hostmaster.{zone_name}"));
    let ttl = req.ttl.unwrap_or(30).clamp(5, 300) as i32;
    let enabled = req.enabled.unwrap_or(true);
    let fallback = normalize_fallback(req.fallback_ip.as_deref()).map_err(bad_request)?;
    // Hosted zones still carrying these names follow the platform to the new set.
    let previous_ns: Vec<String> = crate::repository::platform_ns_entries(&state.pool)
        .await
        .map(|rows| rows.into_iter().map(|r| r.name).collect())
        .unwrap_or_default();

    let mut tx = state.pool.begin().await.map_err(|e| {
        tracing::error!(error = %e, "begin dns settings tx");
        db_error()
    })?;

    // Demote any other schedule zones so there is only one.
    sqlx::query("UPDATE dns_zones SET kind = 'hosted', updated_at = NOW() WHERE kind = 'schedule'")
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "demote old schedule zones");
            db_error()
        })?;

    let existing: Option<(Uuid, i64)> = sqlx::query_as(
        "SELECT id, serial FROM dns_zones WHERE name = $1",
    )
    .bind(&zone_name)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "lookup dns zone");
        db_error()
    })?;

    let zone_id = if let Some((id, serial)) = existing {
        sqlx::query(
            "UPDATE dns_zones
             SET kind = 'schedule',
                 soa_mname = $1,
                 soa_rname = $2,
                 ttl_default = $3,
                 serial = $4,
                 enabled = $5,
                 updated_at = NOW()
             WHERE id = $6",
        )
        .bind(&soa_mname)
        .bind(&soa_rname)
        .bind(ttl)
        .bind(serial.saturating_add(1))
        .bind(enabled)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "update dns zone");
            db_error()
        })?;
        id
    } else {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO dns_zones
                (name, kind, soa_mname, soa_rname, ttl_default, serial, enabled)
             VALUES ($1, 'schedule', $2, $3, $4, 1, $5)
             RETURNING id",
        )
        .bind(&zone_name)
        .bind(&soa_mname)
        .bind(&soa_rname)
        .bind(ttl)
        .bind(enabled)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "insert dns zone");
            db_error()
        })?;
        id
    };

    replace_ns(&mut tx, zone_id, &ns).await?;

    sqlx::query(
        "INSERT INTO dns_settings (id, schedule_zone_id, fallback_ip, updated_at)
         VALUES (1, $1, $2, NOW())
         ON CONFLICT (id) DO UPDATE
         SET schedule_zone_id = EXCLUDED.schedule_zone_id,
             fallback_ip = EXCLUDED.fallback_ip,
             updated_at = NOW()",
    )
    .bind(zone_id)
    .bind(fallback.as_deref())
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "upsert dns settings");
        db_error()
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!(error = %e, "commit dns settings");
        db_error()
    })?;

    // Register authoritative nodes from NS/glue so operators manage identity
    // in the console — not via DNS_PUBLIC_IP in dns.env.
    let ns_pairs: Vec<(String, String)> = ns
        .iter()
        .map(|e| (e.name.clone(), e.ip.clone()))
        .collect();
    if let Err(e) = crate::service::dns_probe::upsert_from_ns(&state.pool, &ns_pairs).await {
        tracing::warn!(error = %e, "upsert dns nodes from schedule NS");
    }
    // Keep hosted zones that serve the platform NS aligned with the schedule zone.
    match crate::repository::propagate_schedule_ns_to_ns_zones(&state.pool, &previous_ns).await {
        Ok(n) if n > 0 => tracing::info!(zones = n, "propagated schedule NS to NS-access zones"),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "propagate schedule NS to NS-access zones failed"),
    }

    get_settings(State(state), actor).await
}

pub async fn sync(
    State(state): State<AppState>,
    _auth: DnsAgentAuth,
) -> Result<Json<DnsSyncResponse>, ApiError> {
    let zones: Vec<ZoneRow> = sqlx::query_as(
        "SELECT id, name, kind, soa_mname, soa_rname, ttl_default, serial, enabled
         FROM dns_zones
         WHERE enabled = TRUE
         ORDER BY kind DESC, name",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "dns sync list zones");
        db_error()
    })?;

    let mut out = Vec::with_capacity(zones.len());
    // Same formula as `view_version`, over the rows served here.
    let mut version: i64 = 0;
    let mut schedule_zone = None;
    for zone in zones {
        version = version.saturating_add(zone.serial);
        if zone.kind == "schedule" {
            schedule_zone = Some(zone.name.clone());
        }
        let ns_rows = sqlx::query_as::<_, NsRow>(
            "SELECT name, ip, sort_order FROM dns_zone_ns
             WHERE zone_id = $1
             ORDER BY sort_order, name",
        )
        .bind(zone.id)
        .fetch_all(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "dns sync list ns");
            db_error()
        })?;
        #[derive(FromRow)]
        struct RecRow {
            name: String,
            rtype: String,
            content: String,
            ttl: i32,
            priority: Option<i32>,
            cdn_proxied: bool,
            line_code: Option<String>,
        }
        let rec_rows = sqlx::query_as::<_, RecRow>(
            r#"
            SELECT r.name, r.rtype, r.content, r.ttl, r.priority, r.cdn_proxied,
                   g.code AS line_code
            FROM dns_records r
            LEFT JOIN geo_regions g ON g.id = r.line_id AND g.enabled = TRUE
            WHERE r.zone_id = $1 AND r.enabled = TRUE
            ORDER BY r.name, r.rtype
            "#,
        )
        .bind(zone.id)
        .fetch_all(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "dns sync list records");
            db_error()
        })?;
        out.push(SyncZone {
            id: zone.id,
            name: zone.name,
            kind: zone.kind,
            soa_mname: zone.soa_mname,
            soa_rname: zone.soa_rname,
            ttl: zone.ttl_default.clamp(5, 300) as u32,
            serial: zone.serial,
            ns: ns_rows
                .into_iter()
                .map(|r| SyncNs {
                    name: r.name,
                    ip: r.ip,
                })
                .collect(),
            records: rec_rows
                .into_iter()
                .map(|r| SyncRecord {
                    name: r.name,
                    rtype: r.rtype,
                    content: r.content,
                    ttl: r.ttl.clamp(5, 86400) as u32,
                    priority: r.priority,
                    cdn_proxied: r.cdn_proxied,
                    line_code: r.line_code.filter(|c| !c.is_empty()),
                })
                .collect(),
        });
    }

    // Prefer settings pointer for schedule_zone name even if demoted somehow.
    if schedule_zone.is_none() {
        schedule_zone = schedule_zone_name(&state.pool).await.map_err(|e| {
            tracing::error!(error = %e, "dns sync schedule name");
            db_error()
        })?;
    }

    let fallback = fallback_ip(&state.pool).await.map_err(|e| {
        tracing::error!(error = %e, "dns sync fallback");
        db_error()
    })?;

    Ok(Json(DnsSyncResponse {
        serial: version,
        fallback_ip: fallback,
        schedule_zone,
        zones: out,
    }))
}

/// Agent helper: map a client IP to the shared geo_regions line (with default fallback).
pub async fn resolve_line(
    State(state): State<AppState>,
    _auth: DnsAgentAuth,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<ResolveLineResponse>, ApiError> {
    let ip = q
        .get("ip")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorBody {
                    error: "ip required".into(),
                }),
            )
        })?;
    let req = crate::scheduler::ScheduleRequest {
        ip: Some(ip.to_string()),
        ..Default::default()
    };
    // Reuse scheduler geo resolution without scheduling nodes.
    let resp = crate::scheduler::resolve(&state, req, false).await;
    let regions = crate::api::geo_region::load_all(&state.pool)
        .await
        .unwrap_or_default();
    let matched = crate::api::geo_region::pick_region(
        &regions,
        &resp.client.continent,
        &resp.client.country,
        &resp.client.province,
        &resp.client.city,
        &resp.client.isp,
    );
    let line = matched
        .or_else(|| regions.iter().find(|r| r.is_default))
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorBody {
                    error: "no geo regions configured".into(),
                }),
            )
        })?;
    Ok(Json(ResolveLineResponse {
        code: line.code.clone(),
        name: line.name.clone(),
        is_default: line.is_default || line.kind == "default",
        country: resp.client.country.clone(),
        continent: resp.client.continent.clone(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_common_zone_names() {
        assert_eq!(
            normalize_zone_name("Edge.Example.COM."),
            Ok("edge.example.com".into())
        );
        assert!(normalize_zone_name("localhost").is_err());
        assert!(normalize_zone_name("").is_err());
    }

    #[test]
    fn expands_short_ns_names() {
        assert_eq!(
            normalize_ns_name("ns1", "edge.example.com"),
            Ok("ns1.edge.example.com".into())
        );
        assert_eq!(
            normalize_ns_name("ns1.edge.example.com.", "edge.example.com"),
            Ok("ns1.edge.example.com".into())
        );
    }
}
