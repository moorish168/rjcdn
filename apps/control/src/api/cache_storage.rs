//! Node cache storage policies: disk dir / capacity (not path/TTL rules).

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::middleware::authz::Actor;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: String,
}
type ApiError = (StatusCode, Json<ErrorBody>);

fn bad_request(e: impl Into<String>) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(ErrorBody { error: e.into() }))
}
fn forbidden() -> ApiError {
    (StatusCode::FORBIDDEN, Json(ErrorBody { error: "forbidden".into() }))
}
fn db_error() -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorBody {
            error: "database error".into(),
        }),
    )
}
fn not_found() -> ApiError {
    (StatusCode::NOT_FOUND, Json(ErrorBody { error: "not found".into() }))
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct StoragePolicyDto {
    pub id: Uuid,
    pub name: String,
    pub note: String,
    pub enabled: bool,
    pub large_cache_enabled: bool,
    pub disk_dir: String,
    pub disk_max_bytes: i64,
    pub memory_max_bytes: i64,
    pub node_count: i64,
}

#[derive(Debug, Deserialize)]
pub struct UpsertRequest {
    pub name: Option<String>,
    pub note: Option<String>,
    pub enabled: Option<bool>,
    pub large_cache_enabled: Option<bool>,
    pub disk_dir: Option<String>,
    pub disk_max_bytes: Option<i64>,
    pub memory_max_bytes: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct AssignRequest {
    pub cache_storage_policy_id: Option<String>,
}

const SELECT_COLS: &str = r#"
    p.id, p.name, p.note, p.enabled, p.large_cache_enabled,
    p.disk_dir, p.disk_max_bytes, p.memory_max_bytes,
    (SELECT COUNT(*) FROM edge_nodes n WHERE n.cache_storage_policy_id = p.id) AS node_count
"#;

pub async fn list(
    State(state): State<AppState>,
    actor: Actor,
) -> Result<Json<Vec<StoragePolicyDto>>, ApiError> {
    if !actor.admin() {
        return Err(forbidden());
    }
    let rows = sqlx::query_as::<_, StoragePolicyDto>(&format!(
        "SELECT {SELECT_COLS} FROM cache_storage_policies p ORDER BY p.name"
    ))
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "list cache_storage_policies");
        db_error()
    })?;
    Ok(Json(rows))
}

pub async fn create(
    State(state): State<AppState>,
    actor: Actor,
    Json(req): Json<UpsertRequest>,
) -> Result<(StatusCode, Json<StoragePolicyDto>), ApiError> {
    if !actor.admin() {
        return Err(forbidden());
    }
    let name = req
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad_request("name required"))?
        .to_string();
    let disk_dir = normalize_disk_dir(req.disk_dir.as_deref().unwrap_or(""))?;
    let disk_max = req.disk_max_bytes.unwrap_or(0).max(0);
    let memory_max = req.memory_max_bytes.unwrap_or(0).max(0);
    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO cache_storage_policies
            (name, note, enabled, large_cache_enabled, disk_dir, disk_max_bytes, memory_max_bytes)
        VALUES ($1,$2,$3,$4,$5,$6,$7)
        RETURNING id
        "#,
    )
    .bind(&name)
    .bind(req.note.as_deref().unwrap_or("").trim())
    .bind(req.enabled.unwrap_or(true))
    .bind(req.large_cache_enabled.unwrap_or(true))
    .bind(&disk_dir)
    .bind(disk_max)
    .bind(memory_max)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if e.as_database_error()
            .is_some_and(|d| d.is_unique_violation())
        {
            return bad_request("name already exists");
        }
        tracing::error!(error = %e, "create cache_storage_policy");
        db_error()
    })?;
    let row = sqlx::query_as::<_, StoragePolicyDto>(&format!(
        "SELECT {SELECT_COLS} FROM cache_storage_policies p WHERE p.id = $1"
    ))
    .bind(id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| db_error())?;
    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn update(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
    Json(req): Json<UpsertRequest>,
) -> Result<Json<StoragePolicyDto>, ApiError> {
    if !actor.admin() {
        return Err(forbidden());
    }
    let uid = Uuid::parse_str(&id).map_err(|_| bad_request("invalid id"))?;
    let current = sqlx::query_as::<_, StoragePolicyDto>(&format!(
        "SELECT {SELECT_COLS} FROM cache_storage_policies p WHERE p.id = $1"
    ))
    .bind(uid)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| db_error())?
    .ok_or_else(not_found)?;

    let name = req
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&current.name)
        .to_string();
    let disk_dir = match req.disk_dir.as_deref() {
        Some(d) => normalize_disk_dir(d)?,
        None => current.disk_dir.clone(),
    };
    sqlx::query(
        r#"
        UPDATE cache_storage_policies SET
            name = $2, note = $3, enabled = $4,
            large_cache_enabled = $5, disk_dir = $6,
            disk_max_bytes = $7, memory_max_bytes = $8,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(uid)
    .bind(&name)
    .bind(
        req.note
            .as_deref()
            .map(str::trim)
            .unwrap_or(&current.note),
    )
    .bind(req.enabled.unwrap_or(current.enabled))
    .bind(
        req.large_cache_enabled
            .unwrap_or(current.large_cache_enabled),
    )
    .bind(&disk_dir)
    .bind(req.disk_max_bytes.unwrap_or(current.disk_max_bytes).max(0))
    .bind(
        req.memory_max_bytes
            .unwrap_or(current.memory_max_bytes)
            .max(0),
    )
    .execute(&state.pool)
    .await
    .map_err(|e| {
        if e.as_database_error()
            .is_some_and(|d| d.is_unique_violation())
        {
            return bad_request("name already exists");
        }
        tracing::error!(error = %e, "update cache_storage_policy");
        db_error()
    })?;
    let row = sqlx::query_as::<_, StoragePolicyDto>(&format!(
        "SELECT {SELECT_COLS} FROM cache_storage_policies p WHERE p.id = $1"
    ))
    .bind(uid)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| db_error())?;

    // Refresh env on bound nodes so capacity/dir take effect on next pull.
    if let Err(e) = apply_policy_to_bound_nodes(&state, &row).await {
        tracing::warn!(error = %e, policy = %row.id, "re-apply storage policy to nodes");
    }
    Ok(Json(row))
}

pub async fn delete(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !actor.admin() {
        return Err(forbidden());
    }
    let uid = Uuid::parse_str(&id).map_err(|_| bad_request("invalid id"))?;
    let bound: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM edge_nodes WHERE cache_storage_policy_id = $1",
    )
    .bind(uid)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| db_error())?;
    if bound > 0 {
        return Err(bad_request(format!(
            "仍有 {bound} 个节点绑定该策略，请先解绑"
        )));
    }
    let r = sqlx::query("DELETE FROM cache_storage_policies WHERE id = $1")
        .bind(uid)
        .execute(&state.pool)
        .await
        .map_err(|_| db_error())?;
    if r.rows_affected() == 0 {
        return Err(not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn assign_node(
    State(state): State<AppState>,
    actor: Actor,
    Path(node_id): Path<String>,
    Json(req): Json<AssignRequest>,
) -> Result<StatusCode, ApiError> {
    if !actor.admin() {
        return Err(forbidden());
    }
    let nid = Uuid::parse_str(&node_id).map_err(|_| bad_request("invalid node id"))?;
    let policy_id = match req.cache_storage_policy_id.as_deref() {
        None | Some("") => None,
        Some(s) => {
            let uid = Uuid::parse_str(s).map_err(|_| bad_request("invalid policy id"))?;
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM cache_storage_policies WHERE id = $1 AND enabled = TRUE)",
            )
            .bind(uid)
            .fetch_one(&state.pool)
            .await
            .map_err(|_| db_error())?;
            if !exists {
                return Err(bad_request("storage policy not found or disabled"));
            }
            Some(uid)
        }
    };
    let r = sqlx::query(
        "UPDATE edge_nodes SET cache_storage_policy_id = $2, updated_at = NOW() WHERE id = $1",
    )
    .bind(nid)
    .bind(policy_id)
    .execute(&state.pool)
    .await
    .map_err(|_| db_error())?;
    if r.rows_affected() == 0 {
        return Err(not_found());
    }
    if let Some(pid) = policy_id {
        let policy = sqlx::query_as::<_, StoragePolicyDto>(&format!(
            "SELECT {SELECT_COLS} FROM cache_storage_policies p WHERE p.id = $1"
        ))
        .bind(pid)
        .fetch_one(&state.pool)
        .await
        .map_err(|_| db_error())?;
        apply_policy_to_node(&state, nid, &policy)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "apply storage policy to node profile");
                internal_apply()
            })?;
    }
    Ok(StatusCode::NO_CONTENT)
}

fn internal_apply() -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorBody {
            error: "绑定成功但写入节点画像失败".into(),
        }),
    )
}

fn normalize_disk_dir(raw: &str) -> Result<String, ApiError> {
    let d = raw.trim();
    if d.is_empty() {
        return Ok("/var/lib/lagran/large-cache".into());
    }
    if !d.starts_with('/') {
        return Err(bad_request("disk_dir must be an absolute path"));
    }
    if d.contains('\0') || d.contains('\n') {
        return Err(bad_request("disk_dir contains invalid characters"));
    }
    Ok(d.to_string())
}

async fn apply_policy_to_bound_nodes(
    state: &AppState,
    policy: &StoragePolicyDto,
) -> Result<(), String> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM edge_nodes WHERE cache_storage_policy_id = $1",
    )
    .bind(policy.id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| e.to_string())?;
    for id in ids {
        apply_policy_to_node(state, id, policy).await?;
    }
    Ok(())
}

async fn apply_policy_to_node(
    state: &AppState,
    node_id: Uuid,
    policy: &StoragePolicyDto,
) -> Result<(), String> {
    let profile: Option<String> =
        sqlx::query_scalar("SELECT COALESCE(edge_profile, '') FROM edge_nodes WHERE id = $1")
            .bind(node_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| e.to_string())?;
    let Some(raw) = profile.filter(|s| !s.trim().is_empty()) else {
        return Ok(());
    };
    let mut value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("edge_profile json: {e}"))?;
    let env = value
        .get("env")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let mut next = upsert_env_line(&env, "LARGE_CACHE_ENABLED", if policy.large_cache_enabled {
        "true"
    } else {
        "false"
    });
    next = upsert_env_line(&next, "LARGE_CACHE_DIR", &policy.disk_dir);
    if policy.disk_max_bytes > 0 {
        next = upsert_env_line(&next, "LARGE_CACHE_MAX_BYTES", &policy.disk_max_bytes.to_string());
    }
    if policy.memory_max_bytes > 0 {
        next = upsert_env_line(
            &next,
            "COMPRESSED_CACHE_BYTES",
            &policy.memory_max_bytes.to_string(),
        );
    }
    if let Some(obj) = value.as_object_mut() {
        obj.insert("env".into(), serde_json::Value::String(next));
    }
    let serialized = value.to_string();
    sqlx::query("UPDATE edge_nodes SET edge_profile = $2, updated_at = NOW() WHERE id = $1")
        .bind(node_id)
        .bind(&serialized)
        .execute(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Insert or replace `KEY=value` in an edge.env body.
fn upsert_env_line(env: &str, key: &str, value: &str) -> String {
    let prefix = format!("{key}=");
    let mut out = Vec::new();
    let mut found = false;
    for line in env.lines() {
        if line.starts_with(&prefix) || line.starts_with(&format!("#{key}=")) {
            if !found {
                out.push(format!("{key}={value}"));
                found = true;
            }
            continue;
        }
        out.push(line.to_string());
    }
    if !found {
        if !out.is_empty() && !out.last().map(|s| s.is_empty()).unwrap_or(true) {
            out.push(String::new());
        }
        out.push(format!("# cache storage policy"));
        out.push(format!("{key}={value}"));
    }
    let mut s = out.join("\n");
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}
