//! Reusable IP black/white lists for WAF policies.

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
pub struct IpListDto {
    pub id: Uuid,
    pub name: String,
    pub list_type: String,
    pub project_id: Option<Uuid>,
    pub note: String,
    pub enabled: bool,
    pub entry_count: i64,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct IpListEntryDto {
    pub id: Uuid,
    pub list_id: Uuid,
    pub cidr: String,
    pub note: String,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateListRequest {
    pub name: String,
    pub list_type: String,
    pub project_id: Option<String>,
    pub note: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateListRequest {
    pub name: Option<String>,
    pub note: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreateEntryRequest {
    pub cidr: String,
    pub note: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEntryRequest {
    pub cidr: Option<String>,
    pub note: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct BindListRequest {
    pub list_id: String,
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct BindingDto {
    pub id: Uuid,
    pub domain_id: Uuid,
    pub list_id: Uuid,
    pub list_name: String,
    pub list_type: String,
    pub enabled: bool,
}

fn normalize_type(t: &str) -> Result<String, ApiError> {
    match t.trim().to_ascii_lowercase().as_str() {
        "black" | "deny" | "block" => Ok("black".into()),
        "white" | "allow" => Ok("white".into()),
        _ => Err(bad_request("list_type must be black or white")),
    }
}

fn normalize_cidr(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(bad_request("cidr required"));
    }
    // Accept a bare IP or CIDR. The prefix must fit the address family: the
    // edge parses every entry, and one bad CIDR must not reach it.
    if let Some((ip, prefix)) = s.split_once('/') {
        let ip: std::net::IpAddr = ip
            .trim()
            .parse()
            .map_err(|_| bad_request("invalid CIDR address"))?;
        let prefix: u8 = prefix
            .trim()
            .parse()
            .map_err(|_| bad_request("invalid CIDR prefix"))?;
        let max = if ip.is_ipv4() { 32 } else { 128 };
        if prefix > max {
            return Err(bad_request("CIDR prefix is too long for this address family"));
        }
        Ok(format!("{ip}/{prefix}"))
    } else {
        s.parse::<std::net::IpAddr>()
            .map(|ip| ip.to_string())
            .map_err(|_| bad_request("invalid IP or CIDR"))
    }
}

async fn actor_project_ids(state: &AppState, actor: &Actor) -> Vec<Uuid> {
    crate::repository::list_projects_for_user(&state.pool, actor.id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.id)
        .collect()
}

pub async fn list(
    State(state): State<AppState>,
    actor: Actor,
) -> Result<Json<Vec<IpListDto>>, ApiError> {
    let rows = if actor.is_admin {
        sqlx::query_as::<_, IpListDto>(
            r#"
            SELECT l.id, l.name, l.list_type, l.project_id, l.note, l.enabled,
                   (SELECT COUNT(*) FROM ip_list_entries e WHERE e.list_id = l.id) AS entry_count
            FROM ip_lists l
            ORDER BY l.name
            "#,
        )
        .fetch_all(&state.pool)
        .await
    } else {
        let pids = actor_project_ids(&state, &actor).await;
        sqlx::query_as::<_, IpListDto>(
            r#"
            SELECT l.id, l.name, l.list_type, l.project_id, l.note, l.enabled,
                   (SELECT COUNT(*) FROM ip_list_entries e WHERE e.list_id = l.id) AS entry_count
            FROM ip_lists l
            WHERE l.project_id IS NULL
               OR l.project_id = ANY($1)
            ORDER BY l.name
            "#,
        )
        .bind(&pids)
        .fetch_all(&state.pool)
        .await
    }
    .map_err(|e| {
        tracing::error!(error = %e, "list ip_lists");
        db_error()
    })?;
    Ok(Json(rows))
}

pub async fn create(
    State(state): State<AppState>,
    actor: Actor,
    Json(req): Json<CreateListRequest>,
) -> Result<(StatusCode, Json<IpListDto>), ApiError> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(bad_request("name required"));
    }
    let list_type = normalize_type(&req.list_type)?;
    let project_id = match req.project_id.as_deref() {
        Some(s) if !s.trim().is_empty() => {
            let pid = Uuid::parse_str(s.trim()).map_err(|_| bad_request("invalid project_id"))?;
            if !actor.is_admin && !actor.can_write_project(&state, pid).await {
                return Err(forbidden());
            }
            Some(pid)
        }
        _ => None,
    };
    if project_id.is_none() && !actor.is_admin {
        return Err(forbidden());
    }
    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO ip_lists (name, list_type, project_id, note, enabled)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id
        "#,
    )
    .bind(name)
    .bind(&list_type)
    .bind(project_id)
    .bind(req.note.as_deref().unwrap_or("").trim())
    .bind(req.enabled.unwrap_or(true))
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if e.as_database_error().is_some_and(|d| d.is_unique_violation()) {
            return bad_request("list name already exists");
        }
        tracing::error!(error = %e, "create ip_list");
        db_error()
    })?;
    let row = get_list(&state, id).await?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn get_list(state: &AppState, id: Uuid) -> Result<IpListDto, ApiError> {
    sqlx::query_as::<_, IpListDto>(
        r#"
        SELECT l.id, l.name, l.list_type, l.project_id, l.note, l.enabled,
               (SELECT COUNT(*) FROM ip_list_entries e WHERE e.list_id = l.id) AS entry_count
        FROM ip_lists l WHERE l.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| db_error())?
    .ok_or_else(not_found)
}

pub async fn update(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
    Json(req): Json<UpdateListRequest>,
) -> Result<Json<IpListDto>, ApiError> {
    let uid = Uuid::parse_str(&id).map_err(|_| bad_request("invalid id"))?;
    let current = get_list(&state, uid).await?;
    ensure_list_write(&state, &actor, &current).await?;
    sqlx::query(
        r#"
        UPDATE ip_lists SET
          name = COALESCE($2, name),
          note = COALESCE($3, note),
          enabled = COALESCE($4, enabled),
          updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(uid)
    .bind(req.name.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(req.note.as_deref())
    .bind(req.enabled)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        if e.as_database_error().is_some_and(|d| d.is_unique_violation()) {
            return bad_request("list name already exists");
        }
        db_error()
    })?;
    Ok(Json(get_list(&state, uid).await?))
}

pub async fn delete(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let uid = Uuid::parse_str(&id).map_err(|_| bad_request("invalid id"))?;
    let current = get_list(&state, uid).await?;
    ensure_list_write(&state, &actor, &current).await?;
    sqlx::query("DELETE FROM ip_lists WHERE id = $1")
        .bind(uid)
        .execute(&state.pool)
        .await
        .map_err(|_| db_error())?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_entries(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
) -> Result<Json<Vec<IpListEntryDto>>, ApiError> {
    let uid = Uuid::parse_str(&id).map_err(|_| bad_request("invalid id"))?;
    let list = get_list(&state, uid).await?;
    ensure_list_read(&state, &actor, &list).await?;
    let rows = sqlx::query_as::<_, IpListEntryDto>(
        "SELECT id, list_id, cidr, note, enabled FROM ip_list_entries WHERE list_id = $1 ORDER BY cidr",
    )
    .bind(uid)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| db_error())?;
    Ok(Json(rows))
}

pub async fn add_entry(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
    Json(req): Json<CreateEntryRequest>,
) -> Result<(StatusCode, Json<IpListEntryDto>), ApiError> {
    let uid = Uuid::parse_str(&id).map_err(|_| bad_request("invalid id"))?;
    let list = get_list(&state, uid).await?;
    ensure_list_write(&state, &actor, &list).await?;
    let cidr = normalize_cidr(&req.cidr)?;
    let row = sqlx::query_as::<_, IpListEntryDto>(
        r#"
        INSERT INTO ip_list_entries (list_id, cidr, note, enabled)
        VALUES ($1, $2, $3, $4)
        RETURNING id, list_id, cidr, note, enabled
        "#,
    )
    .bind(uid)
    .bind(&cidr)
    .bind(req.note.as_deref().unwrap_or("").trim())
    .bind(req.enabled.unwrap_or(true))
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if e.as_database_error().is_some_and(|d| d.is_unique_violation()) {
            return bad_request("CIDR already in list");
        }
        db_error()
    })?;
    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn update_entry(
    State(state): State<AppState>,
    actor: Actor,
    Path((list_id, entry_id)): Path<(String, String)>,
    Json(req): Json<UpdateEntryRequest>,
) -> Result<Json<IpListEntryDto>, ApiError> {
    let lid = Uuid::parse_str(&list_id).map_err(|_| bad_request("invalid list id"))?;
    let eid = Uuid::parse_str(&entry_id).map_err(|_| bad_request("invalid entry id"))?;
    let list = get_list(&state, lid).await?;
    ensure_list_write(&state, &actor, &list).await?;
    let cidr = match req.cidr.as_deref() {
        Some(c) => Some(normalize_cidr(c)?),
        None => None,
    };
    let row = sqlx::query_as::<_, IpListEntryDto>(
        r#"
        UPDATE ip_list_entries SET
          cidr = COALESCE($3, cidr),
          note = COALESCE($4, note),
          enabled = COALESCE($5, enabled)
        WHERE id = $1 AND list_id = $2
        RETURNING id, list_id, cidr, note, enabled
        "#,
    )
    .bind(eid)
    .bind(lid)
    .bind(cidr)
    .bind(req.note.as_deref())
    .bind(req.enabled)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| db_error())?
    .ok_or_else(not_found)?;
    Ok(Json(row))
}

pub async fn delete_entry(
    State(state): State<AppState>,
    actor: Actor,
    Path((list_id, entry_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let lid = Uuid::parse_str(&list_id).map_err(|_| bad_request("invalid list id"))?;
    let eid = Uuid::parse_str(&entry_id).map_err(|_| bad_request("invalid entry id"))?;
    let list = get_list(&state, lid).await?;
    ensure_list_write(&state, &actor, &list).await?;
    let r = sqlx::query("DELETE FROM ip_list_entries WHERE id = $1 AND list_id = $2")
        .bind(eid)
        .bind(lid)
        .execute(&state.pool)
        .await
        .map_err(|_| db_error())?;
    if r.rows_affected() == 0 {
        return Err(not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_bindings(
    State(state): State<AppState>,
    actor: Actor,
    Path(domain_id): Path<String>,
) -> Result<Json<Vec<BindingDto>>, ApiError> {
    let did = Uuid::parse_str(&domain_id).map_err(|_| bad_request("invalid domain id"))?;
    if !actor.can_read_domain(&state, did).await {
        return Err(forbidden());
    }
    let rows = sqlx::query_as::<_, BindingDto>(
        r#"
        SELECT b.id, b.domain_id, b.list_id, l.name AS list_name, l.list_type, b.enabled
        FROM waf_ip_list_bindings b
        JOIN ip_lists l ON l.id = b.list_id
        WHERE b.domain_id = $1
        ORDER BY l.name
        "#,
    )
    .bind(did)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| db_error())?;
    Ok(Json(rows))
}

pub async fn bind(
    State(state): State<AppState>,
    actor: Actor,
    Path(domain_id): Path<String>,
    Json(req): Json<BindListRequest>,
) -> Result<(StatusCode, Json<BindingDto>), ApiError> {
    let did = Uuid::parse_str(&domain_id).map_err(|_| bad_request("invalid domain id"))?;
    if !actor.can_write_domain(&state, did).await {
        return Err(forbidden());
    }
    let lid = Uuid::parse_str(req.list_id.trim()).map_err(|_| bad_request("invalid list_id"))?;
    let list = get_list(&state, lid).await?;
    let domain_project = crate::repository::get_domain(&state.pool, did)
        .await
        .map_err(|_| db_error())?
        .ok_or_else(not_found)?
        .project_id;
    ensure_list_bindable(&state, &actor, &list, domain_project).await?;
    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO waf_ip_list_bindings (domain_id, list_id, enabled)
        VALUES ($1, $2, $3)
        ON CONFLICT (domain_id, list_id) DO UPDATE SET enabled = EXCLUDED.enabled
        RETURNING id
        "#,
    )
    .bind(did)
    .bind(lid)
    .bind(req.enabled.unwrap_or(true))
    .fetch_one(&state.pool)
    .await
    .map_err(|_| db_error())?;
    let row = sqlx::query_as::<_, BindingDto>(
        r#"
        SELECT b.id, b.domain_id, b.list_id, l.name AS list_name, l.list_type, b.enabled
        FROM waf_ip_list_bindings b
        JOIN ip_lists l ON l.id = b.list_id
        WHERE b.id = $1
        "#,
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| db_error())?;
    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn unbind(
    State(state): State<AppState>,
    actor: Actor,
    Path((domain_id, binding_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let did = Uuid::parse_str(&domain_id).map_err(|_| bad_request("invalid domain id"))?;
    let bid = Uuid::parse_str(&binding_id).map_err(|_| bad_request("invalid binding id"))?;
    if !actor.can_write_domain(&state, did).await {
        return Err(forbidden());
    }
    let r = sqlx::query("DELETE FROM waf_ip_list_bindings WHERE id = $1 AND domain_id = $2")
        .bind(bid)
        .bind(did)
        .execute(&state.pool)
        .await
        .map_err(|_| db_error())?;
    if r.rows_affected() == 0 {
        return Err(not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Global lists and lists of the caller's projects are readable; anything
/// else answers 404 so a list id from another tenant reveals nothing.
async fn ensure_list_read(state: &AppState, actor: &Actor, list: &IpListDto) -> Result<(), ApiError> {
    if actor.is_admin {
        return Ok(());
    }
    match list.project_id {
        None => Ok(()),
        Some(pid) if actor.can_read_project(state, pid).await => Ok(()),
        _ => Err(not_found()),
    }
}

/// A list can be bound to a site when it is global or belongs to the site's
/// own project; another project's private list stays private.
async fn ensure_list_bindable(
    state: &AppState,
    actor: &Actor,
    list: &IpListDto,
    domain_project: Option<Uuid>,
) -> Result<(), ApiError> {
    ensure_list_read(state, actor, list).await?;
    if actor.is_admin {
        return Ok(());
    }
    match list.project_id {
        None => Ok(()),
        Some(pid) if Some(pid) == domain_project => Ok(()),
        _ => Err(forbidden()),
    }
}

async fn ensure_list_write(
    state: &AppState,
    actor: &Actor,
    list: &IpListDto,
) -> Result<(), ApiError> {
    if actor.is_admin {
        return Ok(());
    }
    match list.project_id {
        Some(pid) if actor.can_write_project(state, pid).await => Ok(()),
        _ => Err(forbidden()),
    }
}

/// Expand enabled list bindings for a domain into allow/deny CIDRs.
pub async fn expand_for_domain(
    pool: &sqlx::PgPool,
    domain_id: Uuid,
) -> sqlx::Result<(Vec<String>, Vec<String>)> {
    #[derive(FromRow)]
    struct Row {
        list_type: String,
        cidr: String,
    }
    let rows = sqlx::query_as::<_, Row>(
        r#"
        SELECT l.list_type, e.cidr
        FROM waf_ip_list_bindings b
        JOIN ip_lists l ON l.id = b.list_id AND l.enabled = TRUE
        JOIN ip_list_entries e ON e.list_id = l.id AND e.enabled = TRUE
        WHERE b.domain_id = $1 AND b.enabled = TRUE
        "#,
    )
    .bind(domain_id)
    .fetch_all(pool)
    .await?;
    let mut allow = Vec::new();
    let mut deny = Vec::new();
    for r in rows {
        if r.list_type == "white" {
            allow.push(r.cidr);
        } else {
            deny.push(r.cidr);
        }
    }
    Ok((allow, deny))
}

/// `expand_for_domain` for every site at once: domain id → (allow, deny).
pub async fn expand_all(
    pool: &sqlx::PgPool,
) -> sqlx::Result<std::collections::HashMap<Uuid, (Vec<String>, Vec<String>)>> {
    #[derive(FromRow)]
    struct Row {
        domain_id: Uuid,
        list_type: String,
        cidr: String,
    }
    let rows = sqlx::query_as::<_, Row>(
        r#"
        SELECT b.domain_id, l.list_type, e.cidr
        FROM waf_ip_list_bindings b
        JOIN ip_lists l ON l.id = b.list_id AND l.enabled = TRUE
        JOIN ip_list_entries e ON e.list_id = l.id AND e.enabled = TRUE
        WHERE b.enabled = TRUE
        "#,
    )
    .fetch_all(pool)
    .await?;
    let mut out: std::collections::HashMap<Uuid, (Vec<String>, Vec<String>)> =
        std::collections::HashMap::new();
    for r in rows {
        let entry = out.entry(r.domain_id).or_default();
        if r.list_type == "white" {
            entry.0.push(r.cidr);
        } else {
            entry.1.push(r.cidr);
        }
    }
    Ok(out)
}
