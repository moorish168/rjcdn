//! Remote node management (admin only): reusable SSH credentials, node SSH
//! endpoints, batch node creation and remote operations
//! (install / upgrade / reinstall / start / stop / uninstall).

use crate::middleware::authz::Actor;
use crate::repository;
use crate::service::remote::{self, crypto, ops, ssh};
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

type ApiError = (StatusCode, Json<Value>);
type ApiResult<T> = Result<Json<T>, ApiError>;

fn err(status: StatusCode, message: impl Into<String>) -> ApiError {
    (status, Json(json!({ "error": message.into() })))
}

fn db_error(e: sqlx::Error) -> ApiError {
    tracing::error!(error = %e, "remote management database error");
    err(StatusCode::INTERNAL_SERVER_ERROR, "database error")
}

fn admin(actor: &Actor) -> Result<(), ApiError> {
    if actor.admin() {
        Ok(())
    } else {
        Err(err(StatusCode::FORBIDDEN, "admin only"))
    }
}

fn parse_id(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| err(StatusCode::BAD_REQUEST, "invalid id"))
}

async fn audit(state: &AppState, actor: &Actor, action: &str, resource: &str, detail: &str) {
    let _ = repository::insert_audit(&state.pool, &actor.email, action, resource, detail).await;
}

// ---------------------------------------------------------------------------
// SSH credentials
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct GrantDto {
    pub id: String,
    pub name: String,
    pub method: String,
    pub username: String,
    pub use_sudo: bool,
    pub description: String,
    pub has_passphrase: bool,
    pub node_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

fn grant_dto(g: repository::SshGrantRow) -> GrantDto {
    GrantDto {
        id: g.id.to_string(),
        name: g.name,
        method: g.method,
        username: g.username,
        use_sudo: g.use_sudo,
        description: g.description,
        has_passphrase: !g.passphrase_enc.is_empty(),
        node_count: g.node_count,
        created_at: g.created_at.to_rfc3339(),
        updated_at: g.updated_at.to_rfc3339(),
    }
}

#[derive(Deserialize)]
pub struct GrantRequest {
    pub name: String,
    pub method: String,
    pub username: String,
    /// method=password. Empty on update keeps the stored password.
    #[serde(default)]
    pub password: Option<String>,
    /// method=key (PEM / OpenSSH / PuTTY). Empty on update keeps the stored key.
    #[serde(default)]
    pub private_key: Option<String>,
    #[serde(default)]
    pub passphrase: Option<String>,
    #[serde(default)]
    pub use_sudo: bool,
    #[serde(default)]
    pub description: Option<String>,
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|v| !v.is_empty())
}

struct ValidGrant {
    name: String,
    method: String,
    username: String,
    description: String,
}

fn validate_grant(req: &GrantRequest) -> Result<ValidGrant, ApiError> {
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(err(StatusCode::BAD_REQUEST, "名称需要 1-64 个字符"));
    }
    if req.method != "password" && req.method != "key" {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "登录方式只能是 password 或 key",
        ));
    }
    let username = req.username.trim();
    if username.is_empty()
        || username.len() > 64
        || !username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(err(StatusCode::BAD_REQUEST, "用户名不合法"));
    }
    let description = req.description.clone().unwrap_or_default();
    if description.chars().count() > 500 {
        return Err(err(StatusCode::BAD_REQUEST, "备注过长"));
    }
    Ok(ValidGrant {
        name: name.to_string(),
        method: req.method.clone(),
        username: username.to_string(),
        description,
    })
}

fn check_private_key(pem: &str, passphrase: Option<&str>) -> Result<(), ApiError> {
    russh::keys::decode_secret_key(pem, passphrase)
        .map(|_| ())
        .map_err(|e| {
            err(
                StatusCode::BAD_REQUEST,
                format!("私钥无法解析（密码错误或格式不支持）：{e}"),
            )
        })
}

pub async fn list_grants(State(state): State<AppState>, actor: Actor) -> ApiResult<Vec<GrantDto>> {
    admin(&actor)?;
    let rows = repository::list_ssh_grants(&state.pool)
        .await
        .map_err(db_error)?;
    Ok(Json(rows.into_iter().map(grant_dto).collect()))
}

pub async fn create_grant(
    State(state): State<AppState>,
    actor: Actor,
    Json(req): Json<GrantRequest>,
) -> Result<(StatusCode, Json<GrantDto>), ApiError> {
    admin(&actor)?;
    let v = validate_grant(&req)?;
    let jwt = &state.config.jwt_secret;
    let (secret, passphrase) = if v.method == "password" {
        let password =
            non_empty(&req.password).ok_or_else(|| err(StatusCode::BAD_REQUEST, "请填写密码"))?;
        (password.to_string(), String::new())
    } else {
        let key = non_empty(&req.private_key)
            .ok_or_else(|| err(StatusCode::BAD_REQUEST, "请填写私钥"))?;
        let passphrase = req.passphrase.clone().unwrap_or_default();
        check_private_key(key, (!passphrase.is_empty()).then_some(passphrase.as_str()))?;
        (key.to_string(), passphrase)
    };
    let secret_enc =
        crypto::encrypt(jwt, &secret).map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let passphrase_enc =
        crypto::encrypt(jwt, &passphrase).map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let id = repository::create_ssh_grant(
        &state.pool,
        &v.name,
        &v.method,
        &v.username,
        &secret_enc,
        &passphrase_enc,
        req.use_sudo,
        &v.description,
    )
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.constraint() == Some("ssh_grants_name_idx") => {
            err(StatusCode::CONFLICT, "名称已存在")
        }
        _ => db_error(e),
    })?;
    audit(
        &state,
        &actor,
        "ssh_grant.create",
        &v.name,
        &format!("user={} method={}", v.username, v.method),
    )
    .await;
    let row = repository::get_ssh_grant(&state.pool, id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "created grant disappeared",
            )
        })?;
    Ok((StatusCode::CREATED, Json(grant_dto(row))))
}

pub async fn update_grant(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
    Json(req): Json<GrantRequest>,
) -> ApiResult<GrantDto> {
    admin(&actor)?;
    let id = parse_id(&id)?;
    let v = validate_grant(&req)?;
    let existing = repository::get_ssh_grant(&state.pool, id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "SSH 认证不存在"))?;
    let jwt = &state.config.jwt_secret;
    let method_changed = existing.method != v.method;
    let (secret_enc, passphrase_enc) = if v.method == "password" {
        match non_empty(&req.password) {
            Some(password) => (
                crypto::encrypt(jwt, password)
                    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?,
                String::new(),
            ),
            None if !method_changed => (existing.secret_enc.clone(), String::new()),
            None => return Err(err(StatusCode::BAD_REQUEST, "改为密码登录需要填写密码")),
        }
    } else {
        match non_empty(&req.private_key) {
            Some(key) => {
                let passphrase = req.passphrase.clone().unwrap_or_default();
                check_private_key(key, (!passphrase.is_empty()).then_some(passphrase.as_str()))?;
                (
                    crypto::encrypt(jwt, key)
                        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?,
                    crypto::encrypt(jwt, &passphrase)
                        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?,
                )
            }
            None if !method_changed => {
                (existing.secret_enc.clone(), existing.passphrase_enc.clone())
            }
            None => return Err(err(StatusCode::BAD_REQUEST, "改为私钥登录需要填写私钥")),
        }
    };
    repository::update_ssh_grant(
        &state.pool,
        id,
        &v.name,
        &v.method,
        &v.username,
        &secret_enc,
        &passphrase_enc,
        req.use_sudo,
        &v.description,
    )
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.constraint() == Some("ssh_grants_name_idx") => {
            err(StatusCode::CONFLICT, "名称已存在")
        }
        _ => db_error(e),
    })?;
    audit(
        &state,
        &actor,
        "ssh_grant.update",
        &v.name,
        &format!("user={} method={}", v.username, v.method),
    )
    .await;
    let row = repository::get_ssh_grant(&state.pool, id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "SSH 认证不存在"))?;
    Ok(Json(grant_dto(row)))
}

pub async fn delete_grant(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    admin(&actor)?;
    let id = parse_id(&id)?;
    let grant = repository::get_ssh_grant(&state.pool, id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "SSH 认证不存在"))?;
    if grant.node_count > 0 {
        return Err(err(
            StatusCode::CONFLICT,
            format!(
                "还有 {} 个节点在使用该认证，请先给它们换一个认证",
                grant.node_count
            ),
        ));
    }
    repository::delete_ssh_grant(&state.pool, id)
        .await
        .map_err(db_error)?;
    audit(&state, &actor, "ssh_grant.delete", &grant.name, "").await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct TestRequest {
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
}

#[derive(Serialize)]
pub struct TestResponse {
    pub ok: bool,
    pub message: String,
    pub fingerprint: String,
    pub privilege: String,
    pub system: String,
}

/// Log in with a credential and report what the session can do.
pub async fn test_grant(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
    Json(req): Json<TestRequest>,
) -> ApiResult<TestResponse> {
    admin(&actor)?;
    let id = parse_id(&id)?;
    let host = validate_host(&req.host)?;
    let grant = repository::get_ssh_grant(&state.pool, id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "SSH 认证不存在"))?;
    let credentials = remote::credentials_from_grant(&state.config.jwt_secret, &grant)
        .map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    let port = req.port.unwrap_or(22);
    let session = match ssh::connect(&host, port, &credentials, None).await {
        Ok(s) => s,
        Err(e) => {
            return Ok(Json(TestResponse {
                ok: false,
                message: e,
                fingerprint: String::new(),
                privilege: String::new(),
                system: String::new(),
            }))
        }
    };
    let system = session
        .run_user("uname -srm; (. /etc/os-release 2>/dev/null && echo \"$PRETTY_NAME\") || true")
        .await
        .map(|(_, out)| out.trim().replace('\n', " · "))
        .unwrap_or_default();
    let problem = session.root_problem();
    let response = TestResponse {
        ok: problem.is_none(),
        message: problem.unwrap_or_else(|| "连接成功".into()),
        fingerprint: session.fingerprint.clone(),
        privilege: session.root_mode_label().to_string(),
        system,
    };
    session.close().await;
    Ok(Json(response))
}

// ---------------------------------------------------------------------------
// Nodes
// ---------------------------------------------------------------------------

fn validate_host(raw: &str) -> Result<String, ApiError> {
    let host = raw.trim();
    let ok = !host.is_empty()
        && host.len() <= 253
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '_'));
    if ok {
        Ok(host.to_string())
    } else {
        Err(err(
            StatusCode::BAD_REQUEST,
            format!("SSH 地址不合法：{raw}"),
        ))
    }
}

#[derive(Serialize)]
pub struct OpSummary {
    pub id: String,
    pub action: String,
    pub status: String,
    pub error: String,
    pub created_at: String,
    pub finished_at: Option<String>,
}

#[derive(Serialize)]
pub struct RemoteNodeDto {
    pub id: String,
    pub name: String,
    pub region: String,
    pub status: String,
    pub public_ip: String,
    pub version: String,
    pub last_heartbeat: Option<String>,
    pub ssh_host: String,
    pub ssh_port: i32,
    pub grant_id: Option<String>,
    pub grant_name: String,
    pub host_key: String,
    pub last_op: Option<OpSummary>,
}

fn op_summary(o: &repository::NodeOpRow) -> OpSummary {
    OpSummary {
        id: o.id.to_string(),
        action: o.action.clone(),
        status: o.status.clone(),
        error: o.error.clone(),
        created_at: o.created_at.to_rfc3339(),
        finished_at: o.finished_at.map(|t| t.to_rfc3339()),
    }
}

pub async fn list_nodes(
    State(state): State<AppState>,
    actor: Actor,
) -> ApiResult<Vec<RemoteNodeDto>> {
    admin(&actor)?;
    let nodes = repository::list_nodes(&state.pool)
        .await
        .map_err(db_error)?;
    let ssh: HashMap<Uuid, repository::NodeSshRow> = repository::list_node_ssh(&state.pool)
        .await
        .map_err(db_error)?
        .into_iter()
        .map(|s| (s.node_id, s))
        .collect();
    let grants: HashMap<Uuid, String> = repository::list_ssh_grants(&state.pool)
        .await
        .map_err(db_error)?
        .into_iter()
        .map(|g| (g.id, g.name))
        .collect();
    let ops: HashMap<Uuid, repository::NodeOpRow> = repository::latest_node_ops(&state.pool)
        .await
        .map_err(db_error)?
        .into_iter()
        .map(|o| (o.node_id, o))
        .collect();
    let list = nodes
        .into_iter()
        .map(|n| {
            let s = ssh.get(&n.id);
            RemoteNodeDto {
                id: n.id.to_string(),
                name: n.name,
                region: n.region,
                status: n.status,
                public_ip: n.public_ip,
                version: n.version,
                last_heartbeat: n.last_heartbeat.map(|t| t.to_rfc3339()),
                ssh_host: s.map(|s| s.host.clone()).unwrap_or_default(),
                ssh_port: s.map(|s| s.port).unwrap_or(22),
                grant_id: s.and_then(|s| s.grant_id).map(|g| g.to_string()),
                grant_name: s
                    .and_then(|s| s.grant_id)
                    .and_then(|g| grants.get(&g).cloned())
                    .unwrap_or_default(),
                host_key: s.map(|s| s.host_key.clone()).unwrap_or_default(),
                last_op: ops.get(&n.id).map(op_summary),
            }
        })
        .collect();
    Ok(Json(list))
}

#[derive(Deserialize)]
pub struct NodeSshRequest {
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
    pub grant_id: String,
}

async fn require_grant(state: &AppState, raw: &str) -> Result<Uuid, ApiError> {
    let id =
        Uuid::parse_str(raw.trim()).map_err(|_| err(StatusCode::BAD_REQUEST, "请选择 SSH 认证"))?;
    repository::get_ssh_grant(&state.pool, id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "SSH 认证不存在"))?;
    Ok(id)
}

pub async fn set_node_ssh(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
    Json(req): Json<NodeSshRequest>,
) -> ApiResult<Value> {
    admin(&actor)?;
    let node_id = parse_id(&id)?;
    let node = repository::get_node(&state.pool, node_id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "节点不存在"))?;
    let host = validate_host(&req.host)?;
    let grant_id = require_grant(&state, &req.grant_id).await?;
    let port = req.port.unwrap_or(22);
    repository::upsert_node_ssh(&state.pool, node_id, &host, port as i32, Some(grant_id))
        .await
        .map_err(db_error)?;
    audit(
        &state,
        &actor,
        "node.ssh.update",
        &node.name,
        &format!("{host}:{port}"),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete_node_ssh(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    admin(&actor)?;
    let node_id = parse_id(&id)?;
    repository::delete_node_ssh(&state.pool, node_id)
        .await
        .map_err(db_error)?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn reset_host_key(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    admin(&actor)?;
    let node_id = parse_id(&id)?;
    repository::set_node_host_key(&state.pool, node_id, "")
        .await
        .map_err(db_error)?;
    if let Ok(Some(node)) = repository::get_node(&state.pool, node_id).await {
        audit(&state, &actor, "node.ssh.reset_host_key", &node.name, "").await;
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct BatchItem {
    pub name: String,
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
    /// Defaults to `host` when it is an IP address.
    #[serde(default)]
    pub public_ip: Option<String>,
}

#[derive(Deserialize)]
pub struct BatchCreateRequest {
    pub grant_id: String,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub install: bool,
    pub items: Vec<BatchItem>,
}

#[derive(Serialize)]
pub struct BatchItemResult {
    pub name: String,
    pub ok: bool,
    pub node_id: Option<String>,
    pub op_id: Option<String>,
    pub error: Option<String>,
}

/// Create one or many nodes with their SSH endpoint, optionally installing.
pub async fn batch_create(
    State(state): State<AppState>,
    actor: Actor,
    headers: HeaderMap,
    Json(req): Json<BatchCreateRequest>,
) -> ApiResult<Vec<BatchItemResult>> {
    admin(&actor)?;
    if req.items.is_empty() || req.items.len() > 200 {
        return Err(err(StatusCode::BAD_REQUEST, "一次可以添加 1-200 个节点"));
    }
    let grant_id = require_grant(&state, &req.grant_id).await?;
    let region = crate::api::node::validate_region(req.region.as_deref().unwrap_or(""))
        .map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    let params = OpParamsBuilder::new(&state, &headers);
    let install_params = if req.install {
        Some(
            params
                .build(&state, None, true)
                .await
                .map_err(|e| err(StatusCode::BAD_REQUEST, e))?,
        )
    } else {
        None
    };

    let mut results = Vec::with_capacity(req.items.len());
    for item in req.items {
        let name = item.name.trim().to_string();
        let result = create_one(&state, &actor, &name, &item, &region, req.port, grant_id).await;
        match result {
            Ok(node_id) => {
                let mut op_id = None;
                let mut error = None;
                if let Some(ref install_params) = install_params {
                    match queue_op(&state, &actor, node_id, "install", install_params).await {
                        Ok(id) => op_id = Some(id.to_string()),
                        Err(e) => error = Some(e),
                    }
                }
                results.push(BatchItemResult {
                    name,
                    ok: true,
                    node_id: Some(node_id.to_string()),
                    op_id,
                    error,
                });
            }
            Err(e) => results.push(BatchItemResult {
                name,
                ok: false,
                node_id: None,
                op_id: None,
                error: Some(e),
            }),
        }
    }
    state.nodes.invalidate().await;
    Ok(Json(results))
}

async fn create_one(
    state: &AppState,
    actor: &Actor,
    name: &str,
    item: &BatchItem,
    region: &str,
    default_port: Option<u16>,
    grant_id: Uuid,
) -> Result<Uuid, String> {
    if name.is_empty() || name.len() > 64 {
        return Err("名称需要 1-64 个字符".into());
    }
    let host = validate_host(&item.host)
        .map_err(|(_, Json(v))| v["error"].as_str().unwrap_or("").to_string())?;
    let port = item.port.or(default_port).unwrap_or(22);
    let public_ip = item
        .public_ip
        .clone()
        .filter(|ip| !ip.trim().is_empty())
        // An IP SSH address doubles as the public IP, except loopback (an
        // edge on the control host): GeoDNS must never hand out 127.0.0.1.
        .or_else(|| {
            host.parse::<std::net::IpAddr>()
                .ok()
                .filter(|ip| !ip.is_loopback())
                .map(|_| host.clone())
        })
        .unwrap_or_default();
    if !public_ip.is_empty() && public_ip.trim().parse::<std::net::IpAddr>().is_err() {
        return Err(format!("公网 IP 不合法：{public_ip}"));
    }
    if repository::get_node_by_name(&state.pool, name)
        .await
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Err("节点名称已存在".into());
    }
    let token = crate::api::node::new_token();
    let row = repository::create_node(&state.pool, name, region, &public_ip, "", &token)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.constraint() == Some("idx_edge_nodes_name") => {
                "节点名称已存在".to_string()
            }
            _ => "创建节点失败".to_string(),
        })?;
    repository::update_node_meta(
        &state.pool,
        row.id,
        public_ip.trim(),
        "",
        &crate::api::node::normalize_country("CN"),
        "",
        "",
        100,
        0,
        None,
        None,
    )
    .await
    .map_err(|_| "保存节点信息失败".to_string())?;
    repository::upsert_node_ssh(&state.pool, row.id, &host, port as i32, Some(grant_id))
        .await
        .map_err(|_| "保存 SSH 设置失败".to_string())?;
    audit(
        state,
        actor,
        "node.create",
        name,
        &format!("remote ssh={host}:{port} region={region}"),
    )
    .await;
    Ok(row.id)
}

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

struct OpParamsBuilder {
    control_url: String,
}

impl OpParamsBuilder {
    fn new(state: &AppState, headers: &HeaderMap) -> Self {
        Self {
            control_url: crate::api::node::public_base(state, headers),
        }
    }

    async fn build(
        &self,
        state: &AppState,
        version: Option<String>,
        need_repo: bool,
    ) -> Result<String, String> {
        let update_repo = if need_repo {
            Some(crate::api::system::resolve_update_repo(&state.pool).await?)
        } else {
            None
        };
        Ok(serde_json::to_string(&ops::OpParams {
            version,
            control_url: self.control_url.clone(),
            update_repo,
        })
        .unwrap_or_else(|_| "{}".into()))
    }
}

async fn queue_op(
    state: &AppState,
    actor: &Actor,
    node_id: Uuid,
    action: &str,
    params: &str,
) -> Result<Uuid, String> {
    let ssh = repository::get_node_ssh(&state.pool, node_id)
        .await
        .map_err(|e| e.to_string())?;
    match ssh {
        Some(s) if s.grant_id.is_some() => {}
        _ => return Err("节点没有配置 SSH 地址和认证".into()),
    }
    let op_id = repository::create_node_op(&state.pool, node_id, action, params, &actor.email)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "该节点已有进行中的任务".to_string())?;
    if let Ok(Some(node)) = repository::get_node(&state.pool, node_id).await {
        audit(
            state,
            actor,
            &format!("node.remote.{action}"),
            &node.name,
            &format!("op={op_id}"),
        )
        .await;
    }
    ops::spawn(state.clone(), op_id);
    Ok(op_id)
}

#[derive(Deserialize)]
pub struct OpsRequest {
    pub node_ids: Vec<String>,
    pub action: String,
    /// Release tag for install / upgrade / reinstall; latest when omitted.
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Serialize)]
pub struct OpResult {
    pub node_id: String,
    pub op_id: Option<String>,
    pub error: Option<String>,
}

pub async fn create_ops(
    State(state): State<AppState>,
    actor: Actor,
    headers: HeaderMap,
    Json(req): Json<OpsRequest>,
) -> ApiResult<Vec<OpResult>> {
    admin(&actor)?;
    if !ops::ACTIONS.contains(&req.action.as_str()) {
        return Err(err(StatusCode::BAD_REQUEST, "未知操作"));
    }
    if req.node_ids.is_empty() || req.node_ids.len() > 500 {
        return Err(err(StatusCode::BAD_REQUEST, "请选择 1-500 个节点"));
    }
    // Upgrades pin one release for the whole batch, so every node ends up on
    // the same version even if a new release appears mid-way.
    let version = match req.version.as_deref().filter(|v| !v.trim().is_empty()) {
        Some(raw) => Some(
            crate::api::system::validate_tag(raw)
                .ok_or_else(|| err(StatusCode::BAD_REQUEST, "版本号格式不正确，例如 v0.1.12"))?,
        ),
        None if req.action == "upgrade" => Some(
            crate::api::system::latest_release_tag(&state.pool)
                .await
                .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("无法获取最新版本：{e}")))?,
        ),
        None => None,
    };
    let need_repo = matches!(req.action.as_str(), "install" | "reinstall" | "upgrade");
    let params = OpParamsBuilder::new(&state, &headers)
        .build(&state, version, need_repo)
        .await
        .map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    let mut results = Vec::with_capacity(req.node_ids.len());
    for raw in req.node_ids {
        let result = match Uuid::parse_str(&raw) {
            Err(_) => Err("invalid node id".to_string()),
            Ok(node_id) => queue_op(&state, &actor, node_id, &req.action, &params).await,
        };
        results.push(match result {
            Ok(op_id) => OpResult {
                node_id: raw,
                op_id: Some(op_id.to_string()),
                error: None,
            },
            Err(e) => OpResult {
                node_id: raw,
                op_id: None,
                error: Some(e),
            },
        });
    }
    Ok(Json(results))
}

#[derive(Serialize)]
pub struct OpDto {
    pub id: String,
    pub node_id: String,
    pub node_name: String,
    pub action: String,
    pub status: String,
    pub error: String,
    pub created_by: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
    pub params: Value,
}

fn op_dto(o: repository::NodeOpRow, with_log: bool) -> OpDto {
    OpDto {
        id: o.id.to_string(),
        node_id: o.node_id.to_string(),
        node_name: o.node_name,
        action: o.action,
        status: o.status,
        error: o.error,
        created_by: o.created_by,
        created_at: o.created_at.to_rfc3339(),
        started_at: o.started_at.map(|t| t.to_rfc3339()),
        finished_at: o.finished_at.map(|t| t.to_rfc3339()),
        log: with_log.then_some(o.log),
        params: serde_json::from_str(&o.params).unwrap_or(Value::Null),
    }
}

#[derive(Deserialize)]
pub struct OpsQuery {
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn list_ops(
    State(state): State<AppState>,
    actor: Actor,
    Query(q): Query<OpsQuery>,
) -> ApiResult<Vec<OpDto>> {
    admin(&actor)?;
    let node_id = match q.node_id.as_deref().filter(|v| !v.is_empty()) {
        Some(raw) => Some(parse_id(raw)?),
        None => None,
    };
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let rows = repository::list_node_ops(&state.pool, node_id, limit)
        .await
        .map_err(db_error)?;
    Ok(Json(rows.into_iter().map(|o| op_dto(o, false)).collect()))
}

pub async fn get_op(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
) -> ApiResult<OpDto> {
    admin(&actor)?;
    let id = parse_id(&id)?;
    let row = repository::get_node_op(&state.pool, id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "任务不存在"))?;
    Ok(Json(op_dto(row, true)))
}

// ---------------------------------------------------------------------------
// DNS remote install (global ssh_grants; separate dns_node_ssh / dns_node_ops)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct RemoteDnsNodeDto {
    pub id: String,
    pub name: String,
    pub public_ip: String,
    pub listen: String,
    pub status: String,
    pub version: String,
    pub last_seen_at: Option<String>,
    pub ssh_host: String,
    pub ssh_port: i32,
    pub grant_id: Option<String>,
    pub grant_name: String,
    pub host_key: String,
    pub last_op: Option<OpSummary>,
}

pub async fn list_dns_nodes(
    State(state): State<AppState>,
    actor: Actor,
) -> ApiResult<Vec<RemoteDnsNodeDto>> {
    admin(&actor)?;
    let nodes = repository::list_dns_nodes_for_remote(&state.pool)
        .await
        .map_err(db_error)?;
    let ssh: HashMap<Uuid, repository::NodeSshRow> = repository::list_dns_node_ssh(&state.pool)
        .await
        .map_err(db_error)?
        .into_iter()
        .map(|s| (s.node_id, s))
        .collect();
    let grants: HashMap<Uuid, String> = repository::list_ssh_grants(&state.pool)
        .await
        .map_err(db_error)?
        .into_iter()
        .map(|g| (g.id, g.name))
        .collect();
    let ops: HashMap<Uuid, repository::NodeOpRow> = repository::latest_dns_node_ops(&state.pool)
        .await
        .map_err(db_error)?
        .into_iter()
        .map(|o| (o.node_id, o))
        .collect();
    let list = nodes
        .into_iter()
        .map(|n| {
            let s = ssh.get(&n.id);
            RemoteDnsNodeDto {
                id: n.id.to_string(),
                name: n.name,
                public_ip: n.public_ip,
                listen: n.listen,
                status: n.status,
                version: n.version,
                last_seen_at: n.last_seen_at.map(|t| t.to_rfc3339()),
                ssh_host: s.map(|s| s.host.clone()).unwrap_or_default(),
                ssh_port: s.map(|s| s.port).unwrap_or(22),
                grant_id: s.and_then(|s| s.grant_id).map(|g| g.to_string()),
                grant_name: s
                    .and_then(|s| s.grant_id)
                    .and_then(|g| grants.get(&g).cloned())
                    .unwrap_or_default(),
                host_key: s.map(|s| s.host_key.clone()).unwrap_or_default(),
                last_op: ops.get(&n.id).map(op_summary),
            }
        })
        .collect();
    Ok(Json(list))
}

pub async fn set_dns_node_ssh(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
    Json(req): Json<NodeSshRequest>,
) -> ApiResult<Value> {
    admin(&actor)?;
    let node_id = parse_id(&id)?;
    let node = repository::get_dns_node_for_remote(&state.pool, node_id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "DNS 节点不存在"))?;
    let host = validate_host(&req.host)?;
    let grant_id = require_grant(&state, &req.grant_id).await?;
    let port = req.port.unwrap_or(22);
    repository::upsert_dns_node_ssh(&state.pool, node_id, &host, port as i32, Some(grant_id))
        .await
        .map_err(db_error)?;
    audit(
        &state,
        &actor,
        "dns_node.ssh.update",
        &node.name,
        &format!("{host}:{port}"),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete_dns_node_ssh(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    admin(&actor)?;
    let node_id = parse_id(&id)?;
    repository::delete_dns_node_ssh(&state.pool, node_id)
        .await
        .map_err(db_error)?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn reset_dns_host_key(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    admin(&actor)?;
    let node_id = parse_id(&id)?;
    repository::set_dns_node_host_key(&state.pool, node_id, "")
        .await
        .map_err(db_error)?;
    if let Ok(Some(node)) = repository::get_dns_node_for_remote(&state.pool, node_id).await {
        audit(
            &state,
            &actor,
            "dns_node.ssh.reset_host_key",
            &node.name,
            "",
        )
        .await;
    }
    Ok(Json(json!({ "ok": true })))
}

async fn queue_dns_op(
    state: &AppState,
    actor: &Actor,
    node_id: Uuid,
    action: &str,
    params: &str,
) -> Result<Uuid, String> {
    let ssh = repository::get_dns_node_ssh(&state.pool, node_id)
        .await
        .map_err(|e| e.to_string())?;
    match ssh {
        Some(s) if s.grant_id.is_some() => {}
        _ => return Err("节点没有配置 SSH 地址和认证".into()),
    }
    let op_id =
        repository::create_dns_node_op(&state.pool, node_id, action, params, &actor.email)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "该节点已有进行中的任务".to_string())?;
    if let Ok(Some(node)) = repository::get_dns_node_for_remote(&state.pool, node_id).await {
        audit(
            state,
            actor,
            &format!("dns_node.remote.{action}"),
            &node.name,
            &format!("op={op_id}"),
        )
        .await;
    }
    remote::dns_ops::spawn(state.clone(), op_id);
    Ok(op_id)
}

pub async fn create_dns_ops(
    State(state): State<AppState>,
    actor: Actor,
    headers: HeaderMap,
    Json(req): Json<OpsRequest>,
) -> ApiResult<Vec<OpResult>> {
    admin(&actor)?;
    if !remote::dns_ops::actions().contains(&req.action.as_str()) {
        return Err(err(StatusCode::BAD_REQUEST, "未知操作"));
    }
    if req.node_ids.is_empty() || req.node_ids.len() > 500 {
        return Err(err(StatusCode::BAD_REQUEST, "请选择 1-500 个节点"));
    }
    let version = match req.version.as_deref().filter(|v| !v.trim().is_empty()) {
        Some(raw) => Some(
            crate::api::system::validate_tag(raw)
                .ok_or_else(|| err(StatusCode::BAD_REQUEST, "版本号格式不正确，例如 v0.1.12"))?,
        ),
        None if req.action == "upgrade" => Some(
            crate::api::system::latest_release_tag(&state.pool)
                .await
                .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("无法获取最新版本：{e}")))?,
        ),
        None => None,
    };
    let need_repo = matches!(req.action.as_str(), "install" | "reinstall" | "upgrade");
    let params = OpParamsBuilder::new(&state, &headers)
        .build(&state, version, need_repo)
        .await
        .map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    let mut results = Vec::with_capacity(req.node_ids.len());
    for raw in req.node_ids {
        let result = match Uuid::parse_str(&raw) {
            Err(_) => Err("invalid node id".to_string()),
            Ok(node_id) => queue_dns_op(&state, &actor, node_id, &req.action, &params).await,
        };
        results.push(match result {
            Ok(op_id) => OpResult {
                node_id: raw,
                op_id: Some(op_id.to_string()),
                error: None,
            },
            Err(e) => OpResult {
                node_id: raw,
                op_id: None,
                error: Some(e),
            },
        });
    }
    Ok(Json(results))
}

pub async fn list_dns_ops(
    State(state): State<AppState>,
    actor: Actor,
    Query(q): Query<OpsQuery>,
) -> ApiResult<Vec<OpDto>> {
    admin(&actor)?;
    let node_id = match q.node_id.as_deref().filter(|v| !v.is_empty()) {
        Some(raw) => Some(parse_id(raw)?),
        None => None,
    };
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let rows = repository::list_dns_node_ops(&state.pool, node_id, limit)
        .await
        .map_err(db_error)?;
    Ok(Json(rows.into_iter().map(|o| op_dto(o, false)).collect()))
}

pub async fn get_dns_op(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
) -> ApiResult<OpDto> {
    admin(&actor)?;
    let id = parse_id(&id)?;
    let row = repository::get_dns_node_op(&state.pool, id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "任务不存在"))?;
    Ok(Json(op_dto(row, true)))
}
