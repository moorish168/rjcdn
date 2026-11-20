//! DNS node registry (Phase 3).
//!
//! Identity comes from console schedule NS/glue (and optional manual rows).
//! Online/offline is driven by control-plane UDP probes; `/dns/heartbeat`
//! remains an optional enrichment for version/serial stats.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::middleware::agent_auth::DnsAgentAuth;
use crate::middleware::authz::Actor;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: String,
}

type ApiError = (StatusCode, Json<ErrorBody>);

#[derive(Debug, Clone, Serialize)]
pub struct DnsNodeDto {
    pub id: Uuid,
    pub name: String,
    pub public_ip: String,
    pub listen: String,
    pub status: String,
    pub version: String,
    pub git_hash: String,
    pub last_serial: Option<i64>,
    pub cache_hits: Option<i64>,
    pub cache_misses: Option<i64>,
    pub zone_count: Option<i32>,
    pub last_sync_at: Option<chrono::DateTime<Utc>>,
    pub last_seen_at: Option<chrono::DateTime<Utc>>,
    pub enabled: bool,
    pub notes: String,
    pub healthy: bool,
    /// CDN 主域 NS names that point at this node's IP. Several names on one
    /// IP are one server, so they share a row instead of one name winning.
    pub ns_names: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDnsNodeRequest {
    pub name: String,
    pub public_ip: String,
    #[serde(default)]
    pub listen: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDnsNodeRequest {
    pub name: Option<String>,
    pub public_ip: Option<String>,
    pub listen: Option<String>,
    pub notes: Option<String>,
    pub enabled: Option<bool>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DnsHeartbeatRequest {
    /// Optional stable name; falls back to matching by public_ip.
    #[serde(default)]
    pub name: Option<String>,
    pub public_ip: String,
    #[serde(default)]
    pub listen: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub git_hash: Option<String>,
    #[serde(default)]
    pub serial: Option<i64>,
    #[serde(default)]
    pub cache_hits: Option<u64>,
    #[serde(default)]
    pub cache_misses: Option<u64>,
    #[serde(default)]
    pub zone_count: Option<u32>,
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

fn not_found() -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorBody {
            error: "dns node not found".into(),
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

const ONLINE_SECS: i64 = 90;

fn normalize_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("name is required".into());
    }
    if name.chars().count() > 64 {
        return Err("name must be at most 64 characters".into());
    }
    Ok(name.to_string())
}

fn normalize_ip(raw: &str) -> Result<String, String> {
    raw.trim()
        .parse::<std::net::IpAddr>()
        .map(|v| v.to_string())
        .map_err(|_| format!("invalid IP: {}", raw.trim()))
}

fn with_health(mut row: DnsNodeDto) -> DnsNodeDto {
    let fresh = row
        .last_seen_at
        .map(|t| Utc::now() - t < Duration::seconds(ONLINE_SECS))
        .unwrap_or(false);
    row.healthy = row.enabled && row.status != "disabled" && fresh;
    if row.enabled && row.status != "disabled" {
        row.status = if fresh { "online".into() } else { "offline".into() };
    }
    row
}

#[derive(Debug, FromRow)]
struct NodeRow {
    id: Uuid,
    name: String,
    public_ip: String,
    listen: String,
    status: String,
    version: String,
    git_hash: String,
    last_serial: Option<i64>,
    cache_hits: Option<i64>,
    cache_misses: Option<i64>,
    zone_count: Option<i32>,
    last_sync_at: Option<chrono::DateTime<Utc>>,
    last_seen_at: Option<chrono::DateTime<Utc>>,
    enabled: bool,
    notes: String,
}

impl From<NodeRow> for DnsNodeDto {
    fn from(r: NodeRow) -> Self {
        with_health(DnsNodeDto {
            id: r.id,
            name: r.name,
            public_ip: r.public_ip,
            listen: r.listen,
            status: r.status,
            version: r.version,
            git_hash: r.git_hash,
            last_serial: r.last_serial,
            cache_hits: r.cache_hits,
            cache_misses: r.cache_misses,
            zone_count: r.zone_count,
            last_sync_at: r.last_sync_at,
            last_seen_at: r.last_seen_at,
            enabled: r.enabled,
            notes: r.notes,
            healthy: false,
            ns_names: Vec::new(),
        })
    }
}

/// NS names of the CDN 主域, grouped by the IP they point at.
async fn schedule_ns_by_ip(
    pool: &sqlx::PgPool,
) -> Result<std::collections::HashMap<String, Vec<String>>, sqlx::Error> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT ns.name, ns.ip
         FROM dns_settings s
         JOIN dns_zone_ns ns ON ns.zone_id = s.schedule_zone_id
         ORDER BY ns.sort_order, ns.name",
    )
    .fetch_all(pool)
    .await?;
    let mut by_ip: std::collections::HashMap<String, Vec<String>> = Default::default();
    for (name, ip) in rows {
        by_ip.entry(ip).or_default().push(name);
    }
    Ok(by_ip)
}

pub async fn list(
    State(state): State<AppState>,
    actor: Actor,
) -> Result<Json<Vec<DnsNodeDto>>, ApiError> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let rows = sqlx::query_as::<_, NodeRow>(
        "SELECT id, name, public_ip, listen, status, version, git_hash,
                last_serial, cache_hits, cache_misses, zone_count,
                last_sync_at, last_seen_at, enabled, notes
         FROM dns_nodes
         ORDER BY name",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "list dns nodes");
        db_error()
    })?;
    let mut ns_by_ip = schedule_ns_by_ip(&state.pool).await.map_err(|e| {
        tracing::error!(error = %e, "list schedule ns");
        db_error()
    })?;
    Ok(Json(
        rows.into_iter()
            .map(|row| {
                let mut dto = DnsNodeDto::from(row);
                dto.ns_names = ns_by_ip.remove(&dto.public_ip).unwrap_or_default();
                dto
            })
            .collect(),
    ))
}

pub async fn create(
    State(state): State<AppState>,
    actor: Actor,
    Json(req): Json<CreateDnsNodeRequest>,
) -> Result<(StatusCode, Json<DnsNodeDto>), ApiError> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let name = normalize_name(&req.name).map_err(bad_request)?;
    let public_ip = normalize_ip(&req.public_ip).map_err(bad_request)?;
    let listen = req
        .listen
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("0.0.0.0:53")
        .to_string();
    let notes = req.notes.unwrap_or_default();
    let enabled = req.enabled.unwrap_or(true);
    let row = sqlx::query_as::<_, NodeRow>(
        "INSERT INTO dns_nodes (name, public_ip, listen, status, enabled, notes)
         VALUES ($1, $2, $3, 'pending', $4, $5)
         RETURNING id, name, public_ip, listen, status, version, git_hash,
                   last_serial, cache_hits, cache_misses, zone_count,
                   last_sync_at, last_seen_at, enabled, notes",
    )
    .bind(&name)
    .bind(&public_ip)
    .bind(&listen)
    .bind(enabled)
    .bind(notes)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if let Some(db) = e.as_database_error() {
            if db.is_unique_violation() {
                return conflict("dns node name or public_ip already exists");
            }
        }
        tracing::error!(error = %e, "create dns node");
        db_error()
    })?;
    Ok((StatusCode::CREATED, Json(DnsNodeDto::from(row))))
}

pub async fn update(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
    Json(req): Json<UpdateDnsNodeRequest>,
) -> Result<Json<DnsNodeDto>, ApiError> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let uid = Uuid::parse_str(&id).map_err(|_| bad_request("invalid dns node id"))?;
    let current = sqlx::query_as::<_, NodeRow>(
        "SELECT id, name, public_ip, listen, status, version, git_hash,
                last_serial, cache_hits, cache_misses, zone_count,
                last_sync_at, last_seen_at, enabled, notes
         FROM dns_nodes WHERE id = $1",
    )
    .bind(uid)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "get dns node");
        db_error()
    })?
    .ok_or_else(not_found)?;

    let name = match req.name.as_deref() {
        Some(v) => normalize_name(v).map_err(bad_request)?,
        None => current.name,
    };
    let public_ip = match req.public_ip.as_deref() {
        Some(v) => normalize_ip(v).map_err(bad_request)?,
        None => current.public_ip,
    };
    let listen = req
        .listen
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or(current.listen);
    let notes = req.notes.unwrap_or(current.notes);
    let enabled = req.enabled.unwrap_or(current.enabled);
    let status = if !enabled {
        "disabled".to_string()
    } else {
        match req.status.as_deref() {
            Some("disabled") => "disabled".into(),
            Some(s @ ("pending" | "online" | "offline")) => s.to_string(),
            Some(_) => return Err(bad_request("invalid status")),
            None if current.status == "disabled" => "pending".into(),
            None => current.status,
        }
    };

    let row = sqlx::query_as::<_, NodeRow>(
        "UPDATE dns_nodes
         SET name = $1, public_ip = $2, listen = $3, notes = $4,
             enabled = $5, status = $6, updated_at = NOW()
         WHERE id = $7
         RETURNING id, name, public_ip, listen, status, version, git_hash,
                   last_serial, cache_hits, cache_misses, zone_count,
                   last_sync_at, last_seen_at, enabled, notes",
    )
    .bind(&name)
    .bind(&public_ip)
    .bind(&listen)
    .bind(&notes)
    .bind(enabled)
    .bind(&status)
    .bind(uid)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if let Some(db) = e.as_database_error() {
            if db.is_unique_violation() {
                return conflict("dns node name or public_ip already exists");
            }
        }
        tracing::error!(error = %e, "update dns node");
        db_error()
    })?;
    Ok(Json(DnsNodeDto::from(row)))
}

pub async fn delete(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let uid = Uuid::parse_str(&id).map_err(|_| bad_request("invalid dns node id"))?;
    let result = sqlx::query("DELETE FROM dns_nodes WHERE id = $1")
        .bind(uid)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "delete dns node");
            db_error()
        })?;
    if result.rows_affected() == 0 {
        return Err(not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Called by lagran-dns after a successful sync.
pub async fn heartbeat(
    State(state): State<AppState>,
    _auth: DnsAgentAuth,
    Json(req): Json<DnsHeartbeatRequest>,
) -> Result<Json<DnsNodeDto>, ApiError> {
    let public_ip = normalize_ip(&req.public_ip).map_err(bad_request)?;
    let name = req
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let listen = req
        .listen
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("0.0.0.0:53");
    let version = req.version.unwrap_or_default();
    let git_hash = req.git_hash.unwrap_or_default();

    // Prefer match by name, then by public_ip. Auto-register if unknown.
    let existing: Option<Uuid> = if let Some(ref n) = name {
        sqlx::query_scalar("SELECT id FROM dns_nodes WHERE name = $1")
            .bind(n)
            .fetch_optional(&state.pool)
            .await
    } else {
        Ok(None)
    }
    .map_err(|e| {
        tracing::error!(error = %e, "lookup dns node by name");
        db_error()
    })?;

    let existing = if existing.is_some() {
        existing
    } else {
        sqlx::query_scalar("SELECT id FROM dns_nodes WHERE public_ip = $1")
            .bind(&public_ip)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "lookup dns node by ip");
                db_error()
            })?
    };

    let id = if let Some(id) = existing {
        id
    } else {
        let auto_name = name.unwrap_or_else(|| format!("dns-{public_ip}"));
        sqlx::query_scalar(
            "INSERT INTO dns_nodes (name, public_ip, listen, status, enabled)
             VALUES ($1, $2, $3, 'online', TRUE)
             RETURNING id",
        )
        .bind(&auto_name)
        .bind(&public_ip)
        .bind(listen)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            if let Some(db) = e.as_database_error() {
                if db.is_unique_violation() {
                    // Race: another heartbeat inserted; fall through to update by IP.
                    return bad_request("dns node race; retry");
                }
            }
            tracing::error!(error = %e, "auto-register dns node");
            db_error()
        })?
    };

    let row = sqlx::query_as::<_, NodeRow>(
        "UPDATE dns_nodes
         SET public_ip = $1,
             listen = COALESCE(NULLIF($2, ''), listen),
             version = $3,
             git_hash = $4,
             last_serial = $5,
             cache_hits = $6,
             cache_misses = $7,
             zone_count = $8,
             last_sync_at = NOW(),
             last_seen_at = NOW(),
             status = CASE WHEN enabled AND status <> 'disabled' THEN 'online' ELSE status END,
             updated_at = NOW()
         WHERE id = $9
         RETURNING id, name, public_ip, listen, status, version, git_hash,
                   last_serial, cache_hits, cache_misses, zone_count,
                   last_sync_at, last_seen_at, enabled, notes",
    )
    .bind(&public_ip)
    .bind(listen)
    .bind(&version)
    .bind(&git_hash)
    .bind(req.serial)
    .bind(req.cache_hits.map(|v| v as i64))
    .bind(req.cache_misses.map(|v| v as i64))
    .bind(req.zone_count.map(|v| v as i32))
    .bind(id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "dns heartbeat update");
        db_error()
    })?;

    Ok(Json(DnsNodeDto::from(row)))
}
