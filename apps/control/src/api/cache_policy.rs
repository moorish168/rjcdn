//! Reusable cache policies attachable to domains and edge nodes.

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

/// Built-in global default cache conditions (site path/TTL pack).
/// Distinct from node disk `cache_storage_policies`.
pub fn builtin_default_policy_id() -> Uuid {
    Uuid::parse_str("a0000000-0000-4000-8000-000000000001").expect("builtin policy id")
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct CachePolicyDto {
    pub id: Uuid,
    pub name: String,
    pub project_id: Option<Uuid>,
    pub note: String,
    pub enabled: bool,
    #[sqlx(default)]
    pub is_builtin: bool,
    pub rule_count: i64,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct PolicyRuleDto {
    pub id: Uuid,
    pub policy_id: Uuid,
    pub path_pattern: String,
    pub match_type: String,
    pub ttl_seconds: i32,
    pub priority: i32,
    pub action: String,
    pub swr_seconds: i32,
    pub enabled: bool,
    pub match_methods: String,
    pub match_query: String,
    pub browser_ttl_seconds: i32,
    pub ignore_query: Option<bool>,
    pub ignore_cookie: bool,
    pub ignore_key_headers: Option<String>,
    pub match_content_type: String,
    pub note: String,
}

#[derive(Debug, Deserialize)]
pub struct CreatePolicyRequest {
    pub name: String,
    pub project_id: Option<String>,
    pub note: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePolicyRequest {
    pub name: Option<String>,
    pub note: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct PolicyRuleRequest {
    pub path_pattern: Option<String>,
    pub match_type: Option<String>,
    pub ttl_seconds: Option<i32>,
    pub priority: Option<i32>,
    pub action: Option<String>,
    pub swr_seconds: Option<i32>,
    pub enabled: Option<bool>,
    pub match_methods: Option<String>,
    pub match_query: Option<String>,
    pub browser_ttl_seconds: Option<i32>,
    pub ignore_query: Option<bool>,
    pub ignore_cookie: Option<bool>,
    pub ignore_key_headers: Option<String>,
    pub match_content_type: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AssignRequest {
    pub cache_policy_id: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    actor: Actor,
) -> Result<Json<Vec<CachePolicyDto>>, ApiError> {
    let rows = if actor.is_admin {
        sqlx::query_as::<_, CachePolicyDto>(
            r#"
            SELECT p.id, p.name, p.project_id, p.note, p.enabled,
                   COALESCE(p.is_builtin, FALSE) AS is_builtin,
                   (SELECT COUNT(*) FROM cache_policy_rules r WHERE r.policy_id = p.id) AS rule_count
            FROM cache_policies p
            ORDER BY p.is_builtin DESC, p.name
            "#,
        )
        .fetch_all(&state.pool)
        .await
    } else {
        let pids: Vec<Uuid> = crate::repository::list_projects_for_user(&state.pool, actor.id)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.id)
            .collect();
        sqlx::query_as::<_, CachePolicyDto>(
            r#"
            SELECT p.id, p.name, p.project_id, p.note, p.enabled,
                   COALESCE(p.is_builtin, FALSE) AS is_builtin,
                   (SELECT COUNT(*) FROM cache_policy_rules r WHERE r.policy_id = p.id) AS rule_count
            FROM cache_policies p
            WHERE p.project_id IS NULL OR p.project_id = ANY($1)
            ORDER BY p.is_builtin DESC, p.name
            "#,
        )
        .bind(&pids)
        .fetch_all(&state.pool)
        .await
    }
    .map_err(|e| {
        tracing::error!(error = %e, "list cache_policies");
        db_error()
    })?;
    Ok(Json(rows))
}

async fn get_policy(state: &AppState, id: Uuid) -> Result<CachePolicyDto, ApiError> {
    sqlx::query_as::<_, CachePolicyDto>(
        r#"
        SELECT p.id, p.name, p.project_id, p.note, p.enabled,
               COALESCE(p.is_builtin, FALSE) AS is_builtin,
               (SELECT COUNT(*) FROM cache_policy_rules r WHERE r.policy_id = p.id) AS rule_count
        FROM cache_policies p WHERE p.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| db_error())?
    .ok_or_else(not_found)
}

pub async fn create(
    State(state): State<AppState>,
    actor: Actor,
    Json(req): Json<CreatePolicyRequest>,
) -> Result<(StatusCode, Json<CachePolicyDto>), ApiError> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(bad_request("name required"));
    }
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
        INSERT INTO cache_policies (name, project_id, note, enabled)
        VALUES ($1, $2, $3, $4) RETURNING id
        "#,
    )
    .bind(name)
    .bind(project_id)
    .bind(req.note.as_deref().unwrap_or("").trim())
    .bind(req.enabled.unwrap_or(true))
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if e.as_database_error().is_some_and(|d| d.is_unique_violation()) {
            return bad_request("policy name already exists");
        }
        db_error()
    })?;
    Ok((StatusCode::CREATED, Json(get_policy(&state, id).await?)))
}

pub async fn update(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
    Json(req): Json<UpdatePolicyRequest>,
) -> Result<Json<CachePolicyDto>, ApiError> {
    let uid = Uuid::parse_str(&id).map_err(|_| bad_request("invalid id"))?;
    let current = get_policy(&state, uid).await?;
    ensure_write(&state, &actor, &current).await?;
    sqlx::query(
        r#"
        UPDATE cache_policies SET
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
    .map_err(|_| db_error())?;
    Ok(Json(get_policy(&state, uid).await?))
}

pub async fn delete(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let uid = Uuid::parse_str(&id).map_err(|_| bad_request("invalid id"))?;
    let current = get_policy(&state, uid).await?;
    ensure_write(&state, &actor, &current).await?;
    if current.is_builtin || current.id == builtin_default_policy_id() {
        return Err(bad_request("内置默认缓存条件不可删除"));
    }
    sqlx::query("DELETE FROM cache_policies WHERE id = $1 AND is_builtin = FALSE")
        .bind(uid)
        .execute(&state.pool)
        .await
        .map_err(|_| db_error())?;
    Ok(StatusCode::NO_CONTENT)
}

/// Attach the built-in default cache conditions to a newly created site.
pub async fn assign_builtin_default(pool: &sqlx::PgPool, domain_id: Uuid) -> sqlx::Result<()> {
    sqlx::query(
        r#"
        UPDATE domains
        SET cache_policy_id = $2, updated_at = NOW()
        WHERE id = $1 AND cache_policy_id IS NULL
        "#,
    )
    .bind(domain_id)
    .bind(builtin_default_policy_id())
    .execute(pool)
    .await?;
    Ok(())
}

/// Global policies and policies of the caller's projects are readable.
async fn ensure_policy_read(
    state: &AppState,
    actor: &Actor,
    policy: &CachePolicyDto,
) -> Result<(), ApiError> {
    if actor.is_admin {
        return Ok(());
    }
    match policy.project_id {
        None => Ok(()),
        Some(pid) if actor.can_read_project(state, pid).await => Ok(()),
        _ => Err(not_found()),
    }
}

/// A site may use a global policy or one of its own project.
async fn ensure_policy_assignable(
    state: &AppState,
    actor: &Actor,
    policy: &CachePolicyDto,
    domain_project: Option<Uuid>,
) -> Result<(), ApiError> {
    ensure_policy_read(state, actor, policy).await?;
    if actor.is_admin {
        return Ok(());
    }
    match policy.project_id {
        None => Ok(()),
        Some(pid) if Some(pid) == domain_project => Ok(()),
        _ => Err(forbidden()),
    }
}

async fn ensure_write(
    state: &AppState,
    actor: &Actor,
    policy: &CachePolicyDto,
) -> Result<(), ApiError> {
    if actor.is_admin {
        return Ok(());
    }
    match policy.project_id {
        Some(pid) if actor.can_write_project(state, pid).await => Ok(()),
        _ => Err(forbidden()),
    }
}

pub async fn list_rules(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
) -> Result<Json<Vec<PolicyRuleDto>>, ApiError> {
    let uid = Uuid::parse_str(&id).map_err(|_| bad_request("invalid id"))?;
    let policy = get_policy(&state, uid).await?;
    ensure_policy_read(&state, &actor, &policy).await?;
    let rows = sqlx::query_as::<_, PolicyRuleDto>(
        r#"
        SELECT id, policy_id, path_pattern, match_type, ttl_seconds, priority, action,
               swr_seconds, enabled, match_methods, match_query, browser_ttl_seconds,
               ignore_query, ignore_cookie, ignore_key_headers, match_content_type, note
        FROM cache_policy_rules
        WHERE policy_id = $1
        ORDER BY priority DESC, path_pattern
        "#,
    )
    .bind(uid)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| db_error())?;
    Ok(Json(rows))
}

pub async fn add_rule(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
    Json(req): Json<PolicyRuleRequest>,
) -> Result<(StatusCode, Json<PolicyRuleDto>), ApiError> {
    let uid = Uuid::parse_str(&id).map_err(|_| bad_request("invalid id"))?;
    let policy = get_policy(&state, uid).await?;
    ensure_write(&state, &actor, &policy).await?;
    let path = req
        .path_pattern
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad_request("path_pattern required"))?;
    let action = req.action.as_deref().unwrap_or("cache");
    if !matches!(action, "cache" | "bypass" | "no_store") {
        return Err(bad_request("action must be cache|bypass|no_store"));
    }
    let row = sqlx::query_as::<_, PolicyRuleDto>(
        r#"
        INSERT INTO cache_policy_rules
          (policy_id, path_pattern, match_type, ttl_seconds, priority, action, swr_seconds,
           enabled, match_methods, match_query, browser_ttl_seconds, ignore_query,
           ignore_cookie, ignore_key_headers, match_content_type, note)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)
        RETURNING id, policy_id, path_pattern, match_type, ttl_seconds, priority, action,
                  swr_seconds, enabled, match_methods, match_query, browser_ttl_seconds,
                  ignore_query, ignore_cookie, ignore_key_headers, match_content_type, note
        "#,
    )
    .bind(uid)
    .bind(path)
    .bind(req.match_type.as_deref().unwrap_or("glob"))
    .bind(req.ttl_seconds.unwrap_or(60).clamp(0, 31_536_000))
    .bind(req.priority.unwrap_or(0))
    .bind(action)
    .bind(req.swr_seconds.unwrap_or(0).clamp(0, 31_536_000))
    .bind(req.enabled.unwrap_or(true))
    .bind(req.match_methods.as_deref().unwrap_or(""))
    .bind(req.match_query.as_deref().unwrap_or(""))
    .bind(req.browser_ttl_seconds.unwrap_or(0).clamp(0, 31_536_000))
    .bind(req.ignore_query)
    .bind(req.ignore_cookie.unwrap_or(false))
    .bind(req.ignore_key_headers.as_deref())
    .bind(req.match_content_type.as_deref().unwrap_or(""))
    .bind(req.note.as_deref().unwrap_or(""))
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "add cache policy rule");
        db_error()
    })?;
    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn update_rule(
    State(state): State<AppState>,
    actor: Actor,
    Path((policy_id, rule_id)): Path<(String, String)>,
    Json(req): Json<PolicyRuleRequest>,
) -> Result<Json<PolicyRuleDto>, ApiError> {
    let pid = Uuid::parse_str(&policy_id).map_err(|_| bad_request("invalid policy id"))?;
    let rid = Uuid::parse_str(&rule_id).map_err(|_| bad_request("invalid rule id"))?;
    let policy = get_policy(&state, pid).await?;
    ensure_write(&state, &actor, &policy).await?;
    let row = sqlx::query_as::<_, PolicyRuleDto>(
        r#"
        UPDATE cache_policy_rules SET
          path_pattern = COALESCE($3, path_pattern),
          match_type = COALESCE($4, match_type),
          ttl_seconds = COALESCE($5, ttl_seconds),
          priority = COALESCE($6, priority),
          action = COALESCE($7, action),
          swr_seconds = COALESCE($8, swr_seconds),
          enabled = COALESCE($9, enabled),
          match_methods = COALESCE($10, match_methods),
          match_query = COALESCE($11, match_query),
          browser_ttl_seconds = COALESCE($12, browser_ttl_seconds),
          ignore_query = COALESCE($13, ignore_query),
          ignore_cookie = COALESCE($14, ignore_cookie),
          ignore_key_headers = COALESCE($15, ignore_key_headers),
          match_content_type = COALESCE($16, match_content_type),
          note = COALESCE($17, note)
        WHERE id = $1 AND policy_id = $2
        RETURNING id, policy_id, path_pattern, match_type, ttl_seconds, priority, action,
                  swr_seconds, enabled, match_methods, match_query, browser_ttl_seconds,
                  ignore_query, ignore_cookie, ignore_key_headers, match_content_type, note
        "#,
    )
    .bind(rid)
    .bind(pid)
    .bind(req.path_pattern.as_deref())
    .bind(req.match_type.as_deref())
    .bind(req.ttl_seconds)
    .bind(req.priority)
    .bind(req.action.as_deref())
    .bind(req.swr_seconds)
    .bind(req.enabled)
    .bind(req.match_methods.as_deref())
    .bind(req.match_query.as_deref())
    .bind(req.browser_ttl_seconds)
    .bind(req.ignore_query)
    .bind(req.ignore_cookie)
    .bind(req.ignore_key_headers.as_deref())
    .bind(req.match_content_type.as_deref())
    .bind(req.note.as_deref())
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| db_error())?
    .ok_or_else(not_found)?;
    Ok(Json(row))
}

pub async fn delete_rule(
    State(state): State<AppState>,
    actor: Actor,
    Path((policy_id, rule_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let pid = Uuid::parse_str(&policy_id).map_err(|_| bad_request("invalid policy id"))?;
    let rid = Uuid::parse_str(&rule_id).map_err(|_| bad_request("invalid rule id"))?;
    let policy = get_policy(&state, pid).await?;
    ensure_write(&state, &actor, &policy).await?;
    let r = sqlx::query("DELETE FROM cache_policy_rules WHERE id = $1 AND policy_id = $2")
        .bind(rid)
        .bind(pid)
        .execute(&state.pool)
        .await
        .map_err(|_| db_error())?;
    if r.rows_affected() == 0 {
        return Err(not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn assign_domain(
    State(state): State<AppState>,
    actor: Actor,
    Path(domain_id): Path<String>,
    Json(req): Json<AssignRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let did = Uuid::parse_str(&domain_id).map_err(|_| bad_request("invalid domain id"))?;
    if !actor.can_write_domain(&state, did).await {
        return Err(forbidden());
    }
    let policy_id = parse_optional_uuid(req.cache_policy_id.as_deref())?;
    if let Some(pid) = policy_id {
        let policy = get_policy(&state, pid).await?;
        let domain_project = crate::repository::get_domain(&state.pool, did)
            .await
            .map_err(|_| db_error())?
            .ok_or_else(not_found)?
            .project_id;
        ensure_policy_assignable(&state, &actor, &policy, domain_project).await?;
    }
    sqlx::query("UPDATE domains SET cache_policy_id = $2, updated_at = NOW() WHERE id = $1")
        .bind(did)
        .bind(policy_id)
        .execute(&state.pool)
        .await
        .map_err(|_| db_error())?;
    Ok(Json(serde_json::json!({ "ok": true, "cache_policy_id": policy_id })))
}

pub async fn assign_node(
    State(state): State<AppState>,
    actor: Actor,
    Path(node_id): Path<String>,
    Json(req): Json<AssignRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let nid = Uuid::parse_str(&node_id).map_err(|_| bad_request("invalid node id"))?;
    let policy_id = parse_optional_uuid(req.cache_policy_id.as_deref())?;
    if let Some(pid) = policy_id {
        let _ = get_policy(&state, pid).await?;
    }
    sqlx::query("UPDATE edge_nodes SET cache_policy_id = $2 WHERE id = $1")
        .bind(nid)
        .bind(policy_id)
        .execute(&state.pool)
        .await
        .map_err(|_| db_error())?;
    Ok(Json(serde_json::json!({ "ok": true, "cache_policy_id": policy_id })))
}

fn parse_optional_uuid(raw: Option<&str>) -> Result<Option<Uuid>, ApiError> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => Ok(Some(
            Uuid::parse_str(s).map_err(|_| bad_request("invalid cache_policy_id"))?,
        )),
    }
}

/// Load enabled policy rules for a domain (for config merge).
pub async fn rules_for_domain(
    pool: &sqlx::PgPool,
    domain_id: Uuid,
) -> sqlx::Result<Vec<PolicyRuleDto>> {
    sqlx::query_as::<_, PolicyRuleDto>(
        r#"
        SELECT r.id, r.policy_id, r.path_pattern, r.match_type, r.ttl_seconds, r.priority, r.action,
               r.swr_seconds, r.enabled, r.match_methods, r.match_query, r.browser_ttl_seconds,
               r.ignore_query, r.ignore_cookie, r.ignore_key_headers, r.match_content_type, r.note
        FROM domains d
        JOIN cache_policies p ON p.id = d.cache_policy_id AND p.enabled = TRUE
        JOIN cache_policy_rules r ON r.policy_id = p.id AND r.enabled = TRUE
        WHERE d.id = $1
        ORDER BY r.priority DESC
        "#,
    )
    .bind(domain_id)
    .fetch_all(pool)
    .await
}

/// Cache-policy rules as regular cache rules of `domain_id`, ranked below the
/// site's own rules.
pub fn as_cache_rule(domain_id: Uuid, pr: &PolicyRuleDto) -> crate::repository::CacheRuleRow {
    crate::repository::CacheRuleRow {
        id: pr.id,
        domain_id,
        path_pattern: pr.path_pattern.clone(),
        match_type: pr.match_type.clone(),
        ttl_seconds: pr.ttl_seconds as i64,
        priority: pr.priority.saturating_sub(1000),
        action: pr.action.clone(),
        swr_seconds: pr.swr_seconds as i64,
        enabled: pr.enabled,
        match_methods: pr.match_methods.clone(),
        match_query: pr.match_query.clone(),
        browser_ttl_seconds: pr.browser_ttl_seconds as i64,
        ignore_query: pr.ignore_query,
        ignore_cookie: pr.ignore_cookie,
        ignore_key_headers: pr.ignore_key_headers.clone(),
        match_content_type: pr.match_content_type.clone(),
        note: pr.note.clone(),
    }
}

/// Every enabled policy's enabled rules, and which policy each site uses.
/// One pair of queries for the whole config document instead of one per site.
#[derive(Default)]
pub struct PolicyIndex {
    pub rules_by_policy: std::collections::HashMap<Uuid, Vec<PolicyRuleDto>>,
    pub policy_by_domain: std::collections::HashMap<Uuid, Uuid>,
}

impl PolicyIndex {
    pub async fn load(pool: &sqlx::PgPool) -> sqlx::Result<Self> {
        let rules = sqlx::query_as::<_, PolicyRuleDto>(
            r#"
            SELECT r.id, r.policy_id, r.path_pattern, r.match_type, r.ttl_seconds, r.priority, r.action,
                   r.swr_seconds, r.enabled, r.match_methods, r.match_query, r.browser_ttl_seconds,
                   r.ignore_query, r.ignore_cookie, r.ignore_key_headers, r.match_content_type, r.note
            FROM cache_policy_rules r
            JOIN cache_policies p ON p.id = r.policy_id AND p.enabled = TRUE
            WHERE r.enabled = TRUE
            ORDER BY r.priority DESC
            "#,
        )
        .fetch_all(pool)
        .await?;
        let mut rules_by_policy: std::collections::HashMap<Uuid, Vec<PolicyRuleDto>> =
            std::collections::HashMap::new();
        for rule in rules {
            rules_by_policy.entry(rule.policy_id).or_default().push(rule);
        }
        let assigned: Vec<(Uuid, Uuid)> = sqlx::query_as(
            "SELECT id, cache_policy_id FROM domains WHERE cache_policy_id IS NOT NULL",
        )
        .fetch_all(pool)
        .await?;
        Ok(Self {
            rules_by_policy,
            policy_by_domain: assigned.into_iter().collect(),
        })
    }

    /// Rules for a site: its own policy, else the node's fallback policy.
    pub fn rules_for(
        &self,
        domain_id: Uuid,
        node_policy: Option<Uuid>,
    ) -> Vec<crate::repository::CacheRuleRow> {
        let policy = self.policy_by_domain.get(&domain_id).copied().or(node_policy);
        policy
            .and_then(|id| self.rules_by_policy.get(&id))
            .map(|rules| rules.iter().map(|r| as_cache_rule(domain_id, r)).collect())
            .unwrap_or_default()
    }
}
