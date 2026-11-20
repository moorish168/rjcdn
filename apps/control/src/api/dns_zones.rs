//! Zone and DNS record CRUD (Phase 2).

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::middleware::authz::Actor;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: String,
}

type ApiError = (StatusCode, Json<ErrorBody>);

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ZoneDto {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub soa_mname: String,
    pub soa_rname: String,
    pub ttl_default: i32,
    pub serial: i64,
    pub enabled: bool,
    /// CDN site bound via 整域托管 (`domains.dns_zone_id`), if any.
    #[sqlx(default)]
    pub site_id: Option<Uuid>,
    #[sqlx(default)]
    pub site_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateZoneRequest {
    pub name: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub soa_mname: Option<String>,
    #[serde(default)]
    pub soa_rname: Option<String>,
    #[serde(default)]
    pub ttl_default: Option<u32>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub ns: Vec<NsIn>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateZoneRequest {
    pub soa_mname: Option<String>,
    pub soa_rname: Option<String>,
    pub ttl_default: Option<u32>,
    pub enabled: Option<bool>,
    pub ns: Option<Vec<NsIn>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NsIn {
    pub name: String,
    pub ip: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct RecordDto {
    pub id: Uuid,
    pub zone_id: Uuid,
    pub name: String,
    pub rtype: String,
    pub content: String,
    pub ttl: i32,
    pub priority: Option<i32>,
    pub cdn_proxied: bool,
    pub enabled: bool,
    #[sqlx(default)]
    pub line_id: Option<Uuid>,
    #[sqlx(default)]
    pub line_code: Option<String>,
    #[sqlx(default)]
    pub line_name: Option<String>,
    /// Edge-node-maintained row on the CDN schedule zone (inventory mirror).
    #[sqlx(default)]
    pub managed_by_node_id: Option<Uuid>,
    #[sqlx(default)]
    pub managed_node_name: Option<String>,
    /// Site that owns this hostname record (orange or grey cloud).
    #[sqlx(default)]
    pub managed_by_site_id: Option<Uuid>,
    /// For a site-managed record: the site that owns the name...
    #[sqlx(default)]
    pub site_id: Option<Uuid>,
    #[sqlx(default)]
    pub site_name: Option<String>,
    /// ...and where its edges fetch from, e.g. `http://203.0.113.10:8080`.
    /// When `cdn_proxied`, stored content is a placeholder (0.0.0.0) and DNS
    /// answers with edge IPs — show origin instead. When grey cloud, content
    /// is already the origin target.
    #[sqlx(default)]
    pub origin: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRecordRequest {
    pub name: String,
    pub rtype: String,
    pub content: String,
    #[serde(default)]
    pub ttl: Option<u32>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub cdn_proxied: Option<bool>,
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Shared geo_regions id; omit/null = 默认线路
    #[serde(default)]
    pub line_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRecordRequest {
    pub name: Option<String>,
    pub rtype: Option<String>,
    pub content: Option<String>,
    pub ttl: Option<u32>,
    pub priority: Option<i32>,
    pub cdn_proxied: Option<bool>,
    pub enabled: Option<bool>,
    #[serde(default)]
    pub line_id: Option<Option<String>>,
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

fn not_found(what: &str) -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorBody {
            error: format!("{what} not found"),
        }),
    )
}

fn conflict(error: impl Into<String>) -> ApiError {
    (
        StatusCode::CONFLICT,
        Json(ErrorBody {
            error: error.into(),
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

fn normalize_zone_name(raw: &str) -> Result<String, String> {
    crate::api::dns_admin::normalize_zone_name_pub(raw)
}

fn normalize_record_name(raw: &str, zone: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_end_matches('.').to_ascii_lowercase();
    if trimmed.is_empty() || trimmed == "@" {
        return Ok("@".into());
    }
    if trimmed == zone {
        return Ok("@".into());
    }
    let suffix = format!(".{zone}");
    if let Some(rel) = trimmed.strip_suffix(&suffix) {
        if rel.is_empty() {
            return Ok("@".into());
        }
        return Ok(rel.to_string());
    }
    if trimmed.contains('.') && !trimmed.ends_with(&suffix) && trimmed != zone {
        // Absolute name outside this zone is not allowed.
        if !trimmed.ends_with(zone) {
            return Err("record name must be inside the zone".into());
        }
    }
    for label in trimmed.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err("record name has an invalid label".into());
        }
        if label != "*"
            && !label
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err("record name contains invalid characters".into());
        }
    }
    Ok(trimmed)
}

fn normalize_rtype(raw: &str) -> Result<String, String> {
    let rtype = raw.trim().to_ascii_uppercase();
    match rtype.as_str() {
        "A" | "AAAA" | "CNAME" | "TXT" | "MX" | "NS" => Ok(rtype),
        _ => Err(format!("unsupported record type: {raw}")),
    }
}

fn normalize_content(rtype: &str, raw: &str) -> Result<String, String> {
    let content = raw.trim();
    if content.is_empty() {
        return Err("content is required".into());
    }
    match rtype {
        "A" | "AAAA" => content
            .parse::<std::net::IpAddr>()
            .map(|ip| ip.to_string())
            .map_err(|_| format!("invalid IP for {rtype}: {content}")),
        "CNAME" | "NS" => {
            let name = content.trim_end_matches('.').to_ascii_lowercase();
            if name.is_empty() {
                return Err("target name is required".into());
            }
            Ok(name)
        }
        "TXT" => {
            if content.len() > 2048 {
                return Err("TXT content is too long".into());
            }
            Ok(content.to_string())
        }
        "MX" => {
            let name = content.trim_end_matches('.').to_ascii_lowercase();
            if name.is_empty() {
                return Err("MX target is required".into());
            }
            Ok(name)
        }
        _ => Err("unsupported type".into()),
    }
}

fn normalize_ns_name(raw: &str, zone: &str) -> Result<String, String> {
    let mut name = raw
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if name.is_empty() {
        return Err("NS name is required".into());
    }
    if !name.contains('.') {
        name = format!("{name}.{zone}");
    }
    Ok(name)
}

fn normalize_ip(raw: &str) -> Result<String, String> {
    raw.trim()
        .parse::<std::net::IpAddr>()
        .map(|v| v.to_string())
        .map_err(|_| format!("invalid IP address: {}", raw.trim()))
}

async fn bump_serial(pool: &PgPool, zone_id: Uuid) -> Result<i64, ApiError> {
    sqlx::query_scalar(
        "UPDATE dns_zones
         SET serial = serial + 1, updated_at = NOW()
         WHERE id = $1
         RETURNING serial",
    )
    .bind(zone_id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "bump dns serial");
        db_error()
    })
}

async fn load_zone(pool: &PgPool, id: Uuid) -> Result<ZoneDto, ApiError> {
    sqlx::query_as::<_, ZoneDto>(
        r#"
        SELECT z.id, z.name, z.kind, z.soa_mname, z.soa_rname, z.ttl_default, z.serial, z.enabled,
               d.id AS site_id, d.name AS site_name
        FROM dns_zones z
        LEFT JOIN LATERAL (
            SELECT id, name FROM domains WHERE dns_zone_id = z.id ORDER BY created_at ASC LIMIT 1
        ) d ON TRUE
        WHERE z.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "get dns zone");
        db_error()
    })?
    .ok_or_else(|| not_found("zone"))
}

pub async fn list_zones(
    State(state): State<AppState>,
    actor: Actor,
) -> Result<Json<Vec<ZoneDto>>, ApiError> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let rows = sqlx::query_as::<_, ZoneDto>(
        r#"
        SELECT z.id, z.name, z.kind, z.soa_mname, z.soa_rname, z.ttl_default, z.serial, z.enabled,
               d.id AS site_id, d.name AS site_name
        FROM dns_zones z
        LEFT JOIN LATERAL (
            SELECT id, name FROM domains WHERE dns_zone_id = z.id ORDER BY created_at ASC LIMIT 1
        ) d ON TRUE
        ORDER BY z.kind DESC, z.name
        "#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "list dns zones");
        db_error()
    })?;
    Ok(Json(rows))
}

pub async fn create_zone(
    State(state): State<AppState>,
    actor: Actor,
    Json(req): Json<CreateZoneRequest>,
) -> Result<(StatusCode, Json<ZoneDto>), ApiError> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let name = normalize_zone_name(&req.name).map_err(bad_request)?;
    let kind = req
        .kind
        .as_deref()
        .unwrap_or("hosted")
        .trim()
        .to_ascii_lowercase();
    if kind != "hosted" && kind != "schedule" {
        return Err(bad_request("kind must be hosted or schedule"));
    }
    if kind == "schedule" {
        return Err(bad_request(
            "create the schedule zone via DNS settings, not this endpoint",
        ));
    }
    let soa_mname = req
        .soa_mname
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_end_matches('.').to_ascii_lowercase())
        .unwrap_or_else(|| format!("ns1.{name}"));
    let soa_rname = req
        .soa_rname
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_end_matches('.').to_ascii_lowercase())
        .unwrap_or_else(|| format!("hostmaster.{name}"));
    let ttl = req.ttl_default.unwrap_or(30).clamp(5, 300) as i32;
    let enabled = req.enabled.unwrap_or(true);

    let mut tx = state.pool.begin().await.map_err(|e| {
        tracing::error!(error = %e, "begin create zone");
        db_error()
    })?;

    let zone = sqlx::query_as::<_, ZoneDto>(
        r#"
        INSERT INTO dns_zones (name, kind, soa_mname, soa_rname, ttl_default, serial, enabled)
        VALUES ($1, 'hosted', $2, $3, $4, 1, $5)
        RETURNING id, name, kind, soa_mname, soa_rname, ttl_default, serial, enabled,
                  NULL::uuid AS site_id, NULL::text AS site_name
        "#,
    )
    .bind(&name)
    .bind(&soa_mname)
    .bind(&soa_rname)
    .bind(ttl)
    .bind(enabled)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        if let Some(db) = e.as_database_error() {
            if db.is_unique_violation() {
                return conflict("zone already exists");
            }
        }
        tracing::error!(error = %e, "create dns zone");
        db_error()
    })?;

    for (idx, entry) in req.ns.iter().enumerate() {
        let ns_name = normalize_ns_name(&entry.name, &name).map_err(bad_request)?;
        let ip = normalize_ip(&entry.ip).map_err(bad_request)?;
        sqlx::query(
            "INSERT INTO dns_zone_ns (zone_id, name, ip, sort_order)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(zone.id)
        .bind(ns_name)
        .bind(ip)
        .bind(idx as i32)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "insert zone ns");
            db_error()
        })?;
    }

    tx.commit().await.map_err(|e| {
        tracing::error!(error = %e, "commit create zone");
        db_error()
    })?;
    if req.ns.is_empty() {
        // A zone hosted here is answered by the platform's DNS servers, so it
        // gets their NS set (and SOA primary). Without NS, lagran-dns skips
        // the zone and none of its records are served. Before the CDN 主域
        // exists this is a no-op; saving the CDN 主域 fills the zone in later.
        if let Err(e) = crate::repository::sync_zone_ns_from_platform(&state.pool, zone.id).await {
            tracing::warn!(error = %e, zone = %zone.name, "copy platform NS into new zone");
        }
        return Ok((StatusCode::CREATED, Json(load_zone(&state.pool, zone.id).await?)));
    }
    Ok((StatusCode::CREATED, Json(zone)))
}

pub async fn update_zone(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
    Json(req): Json<UpdateZoneRequest>,
) -> Result<Json<ZoneDto>, ApiError> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let uid = Uuid::parse_str(&id).map_err(|_| bad_request("invalid zone id"))?;
    let current = load_zone(&state.pool, uid).await?;

    let soa_mname = req
        .soa_mname
        .as_deref()
        .map(|s| s.trim().trim_end_matches('.').to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .unwrap_or(current.soa_mname);
    let soa_rname = req
        .soa_rname
        .as_deref()
        .map(|s| s.trim().trim_end_matches('.').to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .unwrap_or(current.soa_rname);
    let ttl = req
        .ttl_default
        .map(|v| v.clamp(5, 300) as i32)
        .unwrap_or(current.ttl_default);
    let enabled = req.enabled.unwrap_or(current.enabled);

    let mut tx = state.pool.begin().await.map_err(|e| {
        tracing::error!(error = %e, "begin update zone");
        db_error()
    })?;

    sqlx::query(
        "UPDATE dns_zones
         SET soa_mname = $1, soa_rname = $2, ttl_default = $3, enabled = $4,
             serial = serial + 1, updated_at = NOW()
         WHERE id = $5",
    )
    .bind(&soa_mname)
    .bind(&soa_rname)
    .bind(ttl)
    .bind(enabled)
    .bind(uid)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "update dns zone");
        db_error()
    })?;

    if let Some(ns) = req.ns.as_ref() {
        sqlx::query("DELETE FROM dns_zone_ns WHERE zone_id = $1")
            .bind(uid)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "clear zone ns");
                db_error()
            })?;
        for (idx, entry) in ns.iter().enumerate() {
            let ns_name = normalize_ns_name(&entry.name, &current.name).map_err(bad_request)?;
            let ip = normalize_ip(&entry.ip).map_err(bad_request)?;
            sqlx::query(
                "INSERT INTO dns_zone_ns (zone_id, name, ip, sort_order)
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(uid)
            .bind(ns_name)
            .bind(ip)
            .bind(idx as i32)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "insert zone ns");
                db_error()
            })?;
        }
    }

    tx.commit().await.map_err(|e| {
        tracing::error!(error = %e, "commit update zone");
        db_error()
    })?;
    Ok(Json(load_zone(&state.pool, uid).await?))
}

pub async fn delete_zone(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let uid = Uuid::parse_str(&id).map_err(|_| bad_request("invalid zone id"))?;
    let zone = load_zone(&state.pool, uid).await?;
    if zone.kind == "schedule" {
        return Err(bad_request(
            "cannot delete the schedule zone here; clear it via DNS settings",
        ));
    }
    sqlx::query("DELETE FROM dns_zones WHERE id = $1")
        .bind(uid)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "delete dns zone");
            db_error()
        })?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_records(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
) -> Result<Json<Vec<RecordDto>>, ApiError> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let uid = Uuid::parse_str(&id).map_err(|_| bad_request("invalid zone id"))?;
    let zone = load_zone(&state.pool, uid).await?;
    let mut rows = sqlx::query_as::<_, RecordDto>(
        r#"
        SELECT r.id, r.zone_id, r.name, r.rtype, r.content, r.ttl, r.priority,
               r.cdn_proxied, r.enabled, r.line_id,
               g.code AS line_code, g.name AS line_name,
               r.managed_by_node_id,
               n.name AS managed_node_name,
               r.managed_by_site_id
        FROM dns_records r
        LEFT JOIN geo_regions g ON g.id = r.line_id
        LEFT JOIN edge_nodes n ON n.id = r.managed_by_node_id
        WHERE r.zone_id = $1
        ORDER BY r.name, r.rtype, r.priority NULLS FIRST, r.content
        "#,
    )
    .bind(uid)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "list dns records");
        db_error()
    })?;
    attach_proxied_sites(&state.pool, &zone.name, &mut rows)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "load sites of proxied records");
            db_error()
        })?;
    Ok(Json(rows))
}

/// Fill `site_id` / `site_name` / `origin` of site-managed records (orange or
/// grey cloud) from the site whose hostname they are.
pub async fn attach_proxied_sites(
    pool: &PgPool,
    zone: &str,
    rows: &mut [RecordDto],
) -> Result<(), sqlx::Error> {
    let zone = zone.trim().trim_end_matches('.').to_ascii_lowercase();
    let fqdn = |name: &str| {
        let name = name.trim().trim_end_matches('.').to_ascii_lowercase();
        if name == "@" || name.is_empty() {
            zone.clone()
        } else {
            format!("{name}.{zone}")
        }
    };
    let names: Vec<String> = rows
        .iter()
        .filter(|r| r.cdn_proxied || r.managed_by_site_id.is_some())
        .map(|r| fqdn(&r.name))
        .collect();
    if names.is_empty() {
        return Ok(());
    }
    #[derive(FromRow)]
    struct SiteRow {
        hostname: String,
        id: Uuid,
        name: String,
        origins: sqlx::types::Json<Vec<crate::repository::DomainOrigin>>,
        origin_host: String,
        origin_port: i32,
        origin_proto: String,
    }
    let sites: Vec<SiteRow> = sqlx::query_as(
        r#"
        SELECT lower(rtrim(btrim(h.hostname), '.')) AS hostname,
               d.id, d.name, d.origins, d.origin_host, d.origin_port, d.origin_proto
        FROM domain_hostnames h
        JOIN domains d ON d.id = h.domain_id
        WHERE lower(rtrim(btrim(h.hostname), '.')) = ANY($1)
        UNION
        SELECT lower(rtrim(btrim(d.name), '.')) AS hostname,
               d.id, d.name, d.origins, d.origin_host, d.origin_port, d.origin_proto
        FROM domains d
        WHERE lower(rtrim(btrim(d.name), '.')) = ANY($1)
        "#,
    )
    .bind(&names)
    .fetch_all(pool)
    .await?;
    let by_host: std::collections::HashMap<&str, &SiteRow> =
        sites.iter().map(|s| (s.hostname.as_str(), s)).collect();
    for row in rows
        .iter_mut()
        .filter(|r| r.cdn_proxied || r.managed_by_site_id.is_some())
    {
        let Some(site) = by_host.get(fqdn(&row.name).as_str()) else {
            continue;
        };
        if let Some(mid) = row.managed_by_site_id {
            if mid != site.id {
                continue;
            }
        }
        row.site_id = Some(site.id);
        row.site_name = Some(site.name.clone());
        row.origin = Some(origin_summary(
            &site.origins.0,
            &site.origin_host,
            site.origin_port,
            &site.origin_proto,
        ));
    }
    Ok(())
}

/// Host the edges fetch from first (priority, then weight) — IP or hostname
/// only. DNS list shows this as the orange-cloud "origin" hint; proto/port
/// belong to the site origin editor, not the DNS value column.
fn origin_summary(
    origins: &[crate::repository::DomainOrigin],
    host: &str,
    _port: i32,
    _proto: &str,
) -> String {
    let mut enabled: Vec<&crate::repository::DomainOrigin> = origins
        .iter()
        .filter(|o| o.enabled && !o.host.trim().is_empty())
        .collect();
    enabled.sort_by_key(|o| (o.priority, std::cmp::Reverse(o.weight)));
    match enabled.first() {
        Some(first) => {
            let shown = first.host.trim().to_string();
            if enabled.len() > 1 {
                format!("{shown} 等 {} 个", enabled.len())
            } else {
                shown
            }
        }
        None => host.trim().to_string(),
    }
}

pub async fn create_record(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
    Json(req): Json<CreateRecordRequest>,
) -> Result<(StatusCode, Json<RecordDto>), ApiError> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let uid = Uuid::parse_str(&id).map_err(|_| bad_request("invalid zone id"))?;
    let zone = load_zone(&state.pool, uid).await?;
    let rtype = normalize_rtype(&req.rtype).map_err(bad_request)?;
    let name = normalize_record_name(&req.name, &zone.name).map_err(bad_request)?;
    let content = normalize_content(&rtype, &req.content).map_err(bad_request)?;
    let ttl = req.ttl.unwrap_or(zone.ttl_default as u32).clamp(5, 86400) as i32;
    let priority = if rtype == "MX" {
        Some(req.priority.unwrap_or(10).clamp(0, 65535))
    } else if req.priority.is_some() {
        return Err(bad_request("priority is only valid for MX"));
    } else {
        None
    };
    // CDN acceleration of a name belongs to its site (整域托管), which writes and
    // removes these records itself. A hand-made proxied record has no site
    // behind it: visitors would reach an edge that does not know the host.
    if req.cdn_proxied == Some(true) {
        return Err(bad_request(
            "CDN 加速由站点管理：在站点里选择「整域托管」，平台会自动写入加速记录",
        ));
    }
    let cdn_proxied = false;
    let enabled = req.enabled.unwrap_or(true);
    let line_id = resolve_line_id(&state, req.line_id.as_deref()).await?;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO dns_records
            (zone_id, name, rtype, content, ttl, priority, cdn_proxied, enabled, line_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id",
    )
    .bind(uid)
    .bind(&name)
    .bind(&rtype)
    .bind(&content)
    .bind(ttl)
    .bind(priority)
    .bind(cdn_proxied)
    .bind(enabled)
    .bind(line_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if let Some(db) = e.as_database_error() {
            if db.is_unique_violation() {
                return conflict("duplicate record");
            }
        }
        tracing::error!(error = %e, "create dns record");
        db_error()
    })?;
    let _ = bump_serial(&state.pool, uid).await?;
    let row = load_record(&state.pool, id).await?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn resolve_line_id(
    state: &AppState,
    raw: Option<&str>,
) -> Result<Option<Uuid>, ApiError> {
    let Some(s) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        // NULL line_id means 默认线路 at resolve time.
        return Ok(None);
    };
    let uid = Uuid::parse_str(s).map_err(|_| bad_request("invalid line_id"))?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM geo_regions WHERE id = $1 AND enabled = TRUE)",
    )
    .bind(uid)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| db_error())?;
    if !exists {
        return Err(bad_request("line_id not found or disabled"));
    }
    Ok(Some(uid))
}

async fn load_record(pool: &sqlx::PgPool, id: Uuid) -> Result<RecordDto, ApiError> {
    sqlx::query_as::<_, RecordDto>(
        r#"
        SELECT r.id, r.zone_id, r.name, r.rtype, r.content, r.ttl, r.priority,
               r.cdn_proxied, r.enabled, r.line_id,
               g.code AS line_code, g.name AS line_name,
               r.managed_by_node_id,
               n.name AS managed_node_name,
               r.managed_by_site_id
        FROM dns_records r
        LEFT JOIN geo_regions g ON g.id = r.line_id
        LEFT JOIN edge_nodes n ON n.id = r.managed_by_node_id
        WHERE r.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|_| db_error())?
    .ok_or_else(|| not_found("record"))
}


pub async fn update_record(
    State(state): State<AppState>,
    actor: Actor,
    Path((zone_id, record_id)): Path<(String, String)>,
    Json(req): Json<UpdateRecordRequest>,
) -> Result<Json<RecordDto>, ApiError> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let zid = Uuid::parse_str(&zone_id).map_err(|_| bad_request("invalid zone id"))?;
    let rid = Uuid::parse_str(&record_id).map_err(|_| bad_request("invalid record id"))?;
    let zone = load_zone(&state.pool, zid).await?;
    let current = load_record(&state.pool, rid).await?;
    if current.zone_id != zid {
        return Err(not_found("record"));
    }
    if current.managed_by_node_id.is_some() {
        return Err(bad_request(
            "边缘节点联动记录由节点上下线自动维护，请在「边缘节点」停用或删除节点",
        ));
    }
    if current.managed_by_site_id.is_some() || current.cdn_proxied {
        return Err(bad_request(
            "站点加速记录请在站点里开关 CDN；关闭加速后解析会自动改为源站",
        ));
    }

    let rtype = match req.rtype.as_deref() {
        Some(v) => normalize_rtype(v).map_err(bad_request)?,
        None => current.rtype.clone(),
    };
    let name = match req.name.as_deref() {
        Some(v) => normalize_record_name(v, &zone.name).map_err(bad_request)?,
        None => current.name.clone(),
    };
    let content = match req.content.as_deref() {
        Some(v) => normalize_content(&rtype, v).map_err(bad_request)?,
        None => {
            if rtype != current.rtype {
                normalize_content(&rtype, &current.content).map_err(bad_request)?
            } else {
                current.content.clone()
            }
        }
    };
    let ttl = req
        .ttl
        .map(|v| v.clamp(5, 86400) as i32)
        .unwrap_or(current.ttl);
    let priority = if rtype == "MX" {
        Some(
            req.priority
                .or(current.priority)
                .unwrap_or(10)
                .clamp(0, 65535),
        )
    } else {
        None
    };
    // The flag is the site's to set (see create_record); edits keep it as is.
    if req.cdn_proxied.is_some_and(|v| v != current.cdn_proxied) {
        return Err(bad_request(
            "CDN 加速由站点管理，不能在解析记录里开启或关闭",
        ));
    }
    let cdn_proxied = current.cdn_proxied;
    if cdn_proxied && !matches!(rtype.as_str(), "A" | "AAAA" | "CNAME") {
        return Err(bad_request("cdn_proxied only applies to A/AAAA/CNAME"));
    }
    let enabled = req.enabled.unwrap_or(current.enabled);
    let line_id = match req.line_id {
        None => current.line_id,
        Some(None) => None,
        Some(Some(ref s)) => resolve_line_id(&state, Some(s)).await?,
    };

    sqlx::query(
        "UPDATE dns_records
         SET name = $1, rtype = $2, content = $3, ttl = $4, priority = $5,
             cdn_proxied = $6, enabled = $7, line_id = $8, updated_at = NOW()
         WHERE id = $9 AND zone_id = $10",
    )
    .bind(&name)
    .bind(&rtype)
    .bind(&content)
    .bind(ttl)
    .bind(priority)
    .bind(cdn_proxied)
    .bind(enabled)
    .bind(line_id)
    .bind(rid)
    .bind(zid)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        if let Some(db) = e.as_database_error() {
            if db.is_unique_violation() {
                return conflict("duplicate record");
            }
        }
        tracing::error!(error = %e, "update dns record");
        db_error()
    })?;
    let _ = bump_serial(&state.pool, zid).await?;
    Ok(Json(load_record(&state.pool, rid).await?))
}

pub async fn delete_record(
    State(state): State<AppState>,
    actor: Actor,
    Path((zone_id, record_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let zid = Uuid::parse_str(&zone_id).map_err(|_| bad_request("invalid zone id"))?;
    let rid = Uuid::parse_str(&record_id).map_err(|_| bad_request("invalid record id"))?;
    let _ = load_zone(&state.pool, zid).await?;
    let current = load_record(&state.pool, rid).await?;
    if current.zone_id != zid {
        return Err(not_found("record"));
    }
    if current.managed_by_node_id.is_some() {
        return Err(bad_request(
            "边缘节点联动记录由节点上下线自动维护，请在「边缘节点」停用或删除节点",
        ));
    }
    let result = sqlx::query("DELETE FROM dns_records WHERE id = $1 AND zone_id = $2")
        .bind(rid)
        .bind(zid)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "delete dns record");
            db_error()
        })?;
    if result.rows_affected() == 0 {
        return Err(not_found("record"));
    }
    let _ = bump_serial(&state.pool, zid).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::origin_summary;
    use crate::repository::DomainOrigin;

    fn origin(host: &str, port: u16, proto: &str, priority: i32, weight: u32, enabled: bool) -> DomainOrigin {
        DomainOrigin {
            host: host.into(),
            port,
            proto: proto.into(),
            weight,
            priority,
            enabled,
            sni: None,
            host_header: None,
        }
    }

    #[test]
    fn proxied_records_show_the_origin_edges_try_first() {
        // DNS value column shows host/IP only (no proto://port).
        assert_eq!(origin_summary(&[], "203.0.113.10", 80, "http"), "203.0.113.10");
        assert_eq!(origin_summary(&[], "203.0.113.10", 8080, "http"), "203.0.113.10");
        // Edge order: lowest priority first, then highest weight; disabled skipped.
        let list = [
            origin("backup.example.net", 443, "https", 10, 100, true),
            origin("203.0.113.20", 8443, "https", 0, 50, true),
            origin("203.0.113.30", 443, "https", 0, 200, true),
            origin("203.0.113.40", 80, "http", -5, 100, false),
        ];
        assert_eq!(origin_summary(&list, "unused", 80, "http"), "203.0.113.30 等 3 个");
        let v6 = [origin("2001:db8::1", 8080, "http", 0, 100, true)];
        assert_eq!(origin_summary(&v6, "", 0, ""), "2001:db8::1");
    }
}
