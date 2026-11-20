//! System settings helpers: branding + release updates.

use crate::middleware::authz::Actor;
use crate::state::AppState;
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Write;
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::Duration;

const DEFAULT_REQUEST: &str = "/var/lib/lagran/update.request";
const DEFAULT_STATUS: &str = "/var/lib/lagran/update.status";

/// Console-managed release token (DB), refreshed on boot and branding save.
static CONSOLE_UPDATE_TOKEN: RwLock<Option<String>> = RwLock::new(None);

fn forbidden() -> (StatusCode, Json<Value>) {
    (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})))
}

fn internal(message: impl Into<String>) -> (StatusCode, Json<Value>) {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": message.into()})))
}

fn bad_request(message: impl Into<String>) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({"error": message.into()})))
}

const DEFAULT_SYSTEM_NAME: &str = "控制台";

#[derive(Debug, Serialize)]
pub struct BrandingDto {
    pub system_name: String,
    /// Release `owner/repo` saved in the console. Empty = unset.
    pub update_repo: String,
    /// What remote installs and the node wizard actually use: the console
    /// value, else `LAGRAN_UPDATE_REPO`. Empty = nothing configured.
    #[serde(default)]
    pub effective_update_repo: String,
    /// Repository this host's root updater downloads from
    /// (`LAGRAN_UPDATE_REPO`, written by install.sh). Self-upgrade uses it.
    #[serde(default)]
    pub host_update_repo: String,
    /// Whether a release access token is configured (DB or env). Never returns
    /// the secret itself.
    #[serde(default)]
    pub update_token_configured: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBrandingRequest {
    pub system_name: Option<String>,
    pub update_repo: Option<String>,
    /// Set a new release token; empty string clears the console-stored token.
    /// Omit to leave unchanged.
    #[serde(default)]
    pub update_token: Option<String>,
    /// Required to change `update_repo` or `update_token`: the repository's
    /// install script runs as root on every node installed or upgraded over SSH.
    #[serde(default)]
    pub current_password: Option<String>,
}

fn normalize_system_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("system_name cannot be empty".into());
    }
    if name.chars().count() > 64 {
        return Err("system_name must be at most 64 characters".into());
    }
    Ok(name.to_string())
}

pub async fn load_console_settings(pool: &sqlx::PgPool) -> BrandingDto {
    #[derive(sqlx::FromRow)]
    struct Row {
        system_name: String,
        #[sqlx(default)]
        update_repo: String,
        #[sqlx(default)]
        update_token: String,
    }
    let row = sqlx::query_as::<_, Row>(
        "SELECT system_name,
                COALESCE(update_repo, '') AS update_repo,
                COALESCE(update_token, '') AS update_token
         FROM console_settings WHERE id = 1",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let host = host_update_repo().unwrap_or_default();
    let (system_name, update_repo, db_token) = match row {
        Some(r) => (
            {
                let t = r.system_name.trim();
                if t.is_empty() {
                    DEFAULT_SYSTEM_NAME.to_string()
                } else {
                    t.to_string()
                }
            },
            r.update_repo.trim().to_string(),
            r.update_token.trim().to_string(),
        ),
        None => (DEFAULT_SYSTEM_NAME.to_string(), String::new(), String::new()),
    };
    set_console_update_token_cache(if db_token.is_empty() {
        None
    } else {
        Some(db_token)
    });
    let effective = if update_repo.is_empty() {
        host.clone()
    } else {
        update_repo.clone()
    };
    BrandingDto {
        system_name,
        update_repo,
        effective_update_repo: effective,
        host_update_repo: host,
        update_token_configured: github_token().is_some(),
    }
}

fn set_console_update_token_cache(token: Option<String>) {
    if let Ok(mut guard) = CONSOLE_UPDATE_TOKEN.write() {
        *guard = token.filter(|t| !t.trim().is_empty());
    }
}

/// Load the console release token into the process cache (call after migrate).
pub async fn refresh_update_token_cache(pool: &sqlx::PgPool) {
    let _ = load_console_settings(pool).await;
}

pub async fn get_branding(
    State(state): State<AppState>,
    _actor: Actor,
) -> Result<Json<BrandingDto>, (StatusCode, Json<Value>)> {
    Ok(Json(load_console_settings(&state.pool).await))
}

pub async fn put_branding(
    State(state): State<AppState>,
    actor: Actor,
    Json(req): Json<UpdateBrandingRequest>,
) -> Result<Json<BrandingDto>, (StatusCode, Json<Value>)> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let current = load_console_settings(&state.pool).await;
    let system_name = match req.system_name.as_deref() {
        Some(raw) => normalize_system_name(raw).map_err(bad_request)?,
        None => current.system_name,
    };
    let update_repo = match req.update_repo.as_deref() {
        Some(raw) => normalize_update_repo(raw).map_err(bad_request)?,
        None => current.update_repo.clone(),
    };
    let token_change = req.update_token.as_ref().map(|t| t.trim().to_string());
    let repo_changed = update_repo != current.update_repo;
    let token_changing = token_change.is_some();
    if repo_changed || token_changing {
        // Re-authenticate: a stolen admin session must not be able to point
        // every SSH-managed node at an attacker's install script, or steal a
        // release token by overwriting it.
        let password = req.current_password.as_deref().unwrap_or("");
        let user = crate::repository::find_user_by_id(&state.pool, actor.id)
            .await
            .map_err(|_| internal("database error"))?
            .ok_or_else(forbidden)?;
        if password.is_empty()
            || !crate::utils::verify_password(password, &user.password_hash).unwrap_or(false)
        {
            return Err(bad_request("修改升级仓库或访问令牌需要输入当前登录密码"));
        }
    }
    if let Some(token) = &token_change {
        if token.chars().count() > 200 {
            return Err(bad_request("update_token must be at most 200 characters"));
        }
    }
    match &token_change {
        Some(token) => {
            sqlx::query(
                r#"
                INSERT INTO console_settings (id, system_name, update_repo, update_token, updated_at)
                VALUES (1, $1, $2, $3, NOW())
                ON CONFLICT (id) DO UPDATE
                SET system_name = EXCLUDED.system_name,
                    update_repo = EXCLUDED.update_repo,
                    update_token = EXCLUDED.update_token,
                    updated_at = NOW()
                "#,
            )
            .bind(&system_name)
            .bind(&update_repo)
            .bind(token)
            .execute(&state.pool)
            .await
        }
        None => {
            sqlx::query(
                r#"
                INSERT INTO console_settings (id, system_name, update_repo, updated_at)
                VALUES (1, $1, $2, NOW())
                ON CONFLICT (id) DO UPDATE
                SET system_name = EXCLUDED.system_name,
                    update_repo = EXCLUDED.update_repo,
                    updated_at = NOW()
                "#,
            )
            .bind(&system_name)
            .bind(&update_repo)
            .execute(&state.pool)
            .await
        }
    }
    .map_err(|e| {
        tracing::error!(error = %e, "update console branding");
        internal("database error")
    })?;
    let token_note = match &token_change {
        Some(t) if t.is_empty() => "; update_token=(cleared)",
        Some(_) => "; update_token=(updated)",
        None => "",
    };
    let _ = crate::repository::insert_audit(
        &state.pool,
        &actor.email,
        "system.branding",
        "console",
        &format!(
            "system_name={system_name}; update_repo={}{token_note}",
            if update_repo.is_empty() {
                "(cleared)"
            } else {
                &update_repo
            }
        ),
    )
    .await;
    Ok(Json(load_console_settings(&state.pool).await))
}

fn normalize_update_repo(raw: &str) -> Result<String, String> {
    let repo = raw.trim().trim_matches('/');
    if repo.is_empty() {
        return Ok(String::new());
    }
    // Accept owner/repo or a github.com URL; store as owner/repo only.
    let repo = repo
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("github.com/")
        .trim_start_matches("www.github.com/");
    let repo = repo.split('/').take(2).collect::<Vec<_>>().join("/");
    if repo.split('/').count() != 2
        || repo.split('/').any(|p| p.is_empty())
        || !repo
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/')
        || repo.len() > 200
    {
        return Err("update_repo must be owner/repo".into());
    }
    Ok(repo)
}

/// Repository this host's root updater downloads from. install.sh writes the
/// same `LAGRAN_UPDATE_REPO` to control.env and update.env, so the control
/// process sees exactly what `update.sh` will use.
pub(crate) fn host_update_repo() -> Option<String> {
    std::env::var("LAGRAN_UPDATE_REPO")
        .ok()
        .and_then(|raw| normalize_update_repo(&raw).ok())
        .filter(|r| !r.is_empty())
}

/// Resolve the configured upgrade source.
///
/// Order: console setting → `LAGRAN_UPDATE_REPO` env (ops override). No hardcoded default.
pub async fn resolve_update_repo(pool: &sqlx::PgPool) -> Result<String, String> {
    let from_db = load_console_settings(pool).await.update_repo;
    if !from_db.is_empty() {
        return Ok(from_db);
    }
    if let Ok(env) = std::env::var("LAGRAN_UPDATE_REPO") {
        let env = env.trim().to_string();
        if !env.is_empty() {
            return normalize_update_repo(&env);
        }
    }
    Err("未配置升级仓库：请在系统设置中填写 owner/repo".into())
}

/// Release access token: console setting first, then `LAGRAN_GITHUB_TOKEN`.
pub(crate) fn github_token() -> Option<String> {
    if let Ok(guard) = CONSOLE_UPDATE_TOKEN.read() {
        if let Some(token) = guard.as_ref().filter(|t| !t.is_empty()) {
            return Some(token.clone());
        }
    }
    std::env::var("LAGRAN_GITHUB_TOKEN")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Console update UI / check require both a saved repository and a token.
pub(crate) fn update_credentials_ready(settings: &BrandingDto) -> bool {
    !settings.update_repo.trim().is_empty() && settings.update_token_configured
}

fn request_path() -> PathBuf {
    std::env::var("LAGRAN_UPDATE_REQUEST_PATH")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_REQUEST))
}

fn status_path() -> PathBuf {
    std::env::var("LAGRAN_UPDATE_STATUS_PATH")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_STATUS))
}

/// Serializes apply-update requests inside this control process. The request
/// file and status file are the cross-process truth; this lock closes the
/// read-check-write race between two simultaneous clicks.
static UPDATE_APPLY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn status_state() -> String {
    std::fs::read_to_string(status_path())
        .ok()
        .and_then(|raw| {
            raw.trim()
                .split('|')
                .next()
                .map(|value| value.trim().to_ascii_lowercase())
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "idle".to_string())
}

fn update_active(state: &str) -> bool {
    matches!(
        state,
        "queued" | "pending" | "downloading" | "installing" | "restarting" | "running"
    )
}

fn updater_available() -> bool {
    let helper = std::env::var("LAGRAN_UPDATE_HELPER")
        .unwrap_or_else(|_| "/opt/lagran/bin/lagran-update".to_string());
    std::path::Path::new(&helper).is_file()
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    body: Option<String>,
}

async fn fetch_latest(repo: &str) -> Result<GithubRelease, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let mut request = client
        .get(&url)
        .header("User-Agent", format!("lagran-control/{}", crate::version::VERSION))
        .header("Accept", "application/vnd.github+json");
    if let Some(token) = github_token() {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|e| format!("GitHub request failed: {e}"))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND && github_token().is_none() {
        return Err(format!(
            "仓库 {repo} 返回 404：私有仓请在系统设置填写访问令牌，或配置 LAGRAN_GITHUB_TOKEN"
        ));
    }
    if !response.status().is_success() {
        return Err(format!("Release API returned HTTP {}", response.status()));
    }
    response
        .json::<GithubRelease>()
        .await
        .map_err(|e| format!("invalid release response: {e}"))
}

fn parse_version(raw: &str) -> Option<(u64, u64, u64)> {
    let core = raw
        .trim()
        .trim_start_matches('v')
        .split(['+', '-'])
        .next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

fn version_gt(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

#[derive(Serialize)]
pub struct UpdateCheckResponse {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub release_url: Option<String>,
    pub published_at: Option<String>,
    pub notes: Option<String>,
    pub updater_available: bool,
    pub error: Option<String>,
}

pub async fn check_update(
    State(state): State<AppState>,
    actor: Actor,
) -> Result<Json<UpdateCheckResponse>, (StatusCode, Json<Value>)> {
    if !actor.admin() {
        return Err(forbidden());
    }
    let available = updater_available();
    let settings = load_console_settings(&state.pool).await;
    if !update_credentials_ready(&settings) {
        return Ok(Json(UpdateCheckResponse {
            current_version: crate::version::VERSION.to_string(),
            latest_version: None,
            update_available: false,
            release_url: None,
            published_at: None,
            notes: None,
            updater_available: available,
            error: Some(
                "请先在「品牌与升级源」填写升级仓库与访问令牌并保存".into(),
            ),
        }));
    }
    let repo = match resolve_update_repo(&state.pool).await {
        Ok(r) => r,
        Err(error) => {
            return Ok(Json(UpdateCheckResponse {
                current_version: crate::version::VERSION.to_string(),
                latest_version: None,
                update_available: false,
                release_url: None,
                published_at: None,
                notes: None,
                updater_available: available,
                error: Some(error),
            }));
        }
    };
    match fetch_latest(&repo).await {
        Ok(release) => {
            let latest = release.tag_name.trim_start_matches('v').to_string();
            Ok(Json(UpdateCheckResponse {
                current_version: crate::version::VERSION.to_string(),
                update_available: version_gt(&latest, crate::version::VERSION),
                latest_version: Some(latest),
                release_url: release.html_url,
                published_at: release.published_at,
                notes: release.body.map(|b| b.chars().take(2000).collect()),
                updater_available: available,
                error: None,
            }))
        }
        Err(error) => Ok(Json(UpdateCheckResponse {
            current_version: crate::version::VERSION.to_string(),
            latest_version: None,
            update_available: false,
            release_url: None,
            published_at: None,
            notes: None,
            updater_available: available,
            error: Some(error),
        })),
    }
}

#[derive(Deserialize)]
pub struct ApplyUpdateRequest {
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Serialize)]
pub struct ApplyUpdateResponse {
    pub queued: bool,
    pub version: String,
    pub message: String,
}

pub(crate) fn validate_tag(raw: &str) -> Option<String> {
    let tag = if raw.trim_start().starts_with('v') {
        raw.trim().to_string()
    } else {
        format!("v{}", raw.trim())
    };
    let rest = tag.strip_prefix('v')?;
    if rest.is_empty() || rest.len() > 64 {
        return None;
    }
    if !rest
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '+')
    {
        return None;
    }
    let core = rest.split(['+', '-']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?;
    let minor = parts.next()?;
    let patch = parts.next()?;
    if parts.next().is_some()
        || major.is_empty()
        || minor.is_empty()
        || patch.is_empty()
        || !major.chars().all(|c| c.is_ascii_digit())
        || !minor.chars().all(|c| c.is_ascii_digit())
        || !patch.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    Some(tag)
}

/// Resolve the latest GitHub release tag, normalized to `vX.Y.Z`.
pub(crate) async fn latest_release_tag(pool: &sqlx::PgPool) -> Result<String, String> {
    let repo = resolve_update_repo(pool).await?;
    let release = fetch_latest(&repo).await?;
    validate_tag(&release.tag_name)
        .ok_or_else(|| format!("invalid release tag from GitHub: {}", release.tag_name))
}

pub async fn apply_update(
    State(state): State<crate::state::AppState>,
    actor: Actor,
    Json(req): Json<ApplyUpdateRequest>,
) -> Result<Json<ApplyUpdateResponse>, (StatusCode, Json<Value>)> {
    if !actor.admin() {
        return Err(forbidden());
    }
    if !updater_available() {
        return Err(internal(
            "updater is not installed; reinstall control with install.sh control",
        ));
    }
    let settings = load_console_settings(&state.pool).await;
    if !update_credentials_ready(&settings) {
        return Err(bad_request(
            "请先在「品牌与升级源」填写升级仓库与访问令牌并保存",
        ));
    }
    let tag = if let Some(raw) = req.version {
        validate_tag(&raw).ok_or_else(|| bad_request("invalid release version"))?
    } else {
        let repo = resolve_update_repo(&state.pool)
            .await
            .map_err(bad_request)?;
        let release = fetch_latest(&repo).await.map_err(internal)?;
        validate_tag(&release.tag_name).ok_or_else(|| bad_request("invalid latest release tag"))?
    };
    // The lock is held only around the status check and request-file write. It
    // must not span an await, otherwise the Axum handler future is !Send.
    {
        let _guard = UPDATE_APPLY_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let update_state = status_state();
        let request = request_path();
        if request.exists() || update_active(&update_state) {
            return Err((
                StatusCode::CONFLICT,
                Json(json!({
                    "error": format!("an update is already queued or running (state: {update_state})"),
                    "state": update_state,
                })),
            ));
        }
        if let Some(parent) = request.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| internal(format!("cannot create update request directory: {e}")))?;
        }
        // Recreate the file so the systemd .path unit always observes a change.
        let _ = std::fs::remove_file(&request);
        // The old single `lagran` account may have left `update.request.tmp`
        // owned by a uid the new `lagran-control` account cannot write. A
        // per-process name plus `create_new` makes a stale file or symlink
        // impossible to trip over, and the error names the directory to check.
        let tmp = request.with_extension(format!("request.tmp.{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_file(&tmp);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| {
                let dir = request
                    .parent()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "/var/lib/lagran".to_string());
                internal(format!(
                    "cannot write update request {}: {e} (the control service user must be able to write {dir}; run: sudo chgrp lagran {dir} && sudo chmod g+rwxs {dir}, or rerun install.sh control)",
                    tmp.display()
                ))
            })?;
        if let Err(e) = file.write_all(format!("{tag}\n").as_bytes()) {
            let _ = std::fs::remove_file(&tmp);
            return Err(internal(format!("cannot write update request: {e}")));
        }
        drop(file);
        if let Err(e) = std::fs::rename(&tmp, &request) {
            let _ = std::fs::remove_file(&tmp);
            return Err(internal(format!("cannot queue update request: {e}")));
        }
    }
    let _ = crate::repository::insert_audit(
        &state.pool,
        &actor.email,
        "system.update",
        &tag,
        "queued release update",
    )
    .await;
    Ok(Json(ApplyUpdateResponse {
        queued: true,
        version: tag.clone(),
        message: format!("update to {tag} queued; the control plane will restart"),
    }))
}

#[derive(Serialize)]
pub struct UpdateStatusResponse {
    pub state: String,
    pub version: Option<String>,
    pub message: Option<String>,
    pub updated_at: Option<String>,
    pub updater_available: bool,
    pub raw: Option<String>,
}

pub async fn update_status(
    actor: Actor,
) -> Result<Json<UpdateStatusResponse>, (StatusCode, Json<Value>)> {
    if !actor.admin() {
        return Err(forbidden());
    }
    let raw = std::fs::read_to_string(status_path()).ok();
    let mut state = "idle".to_string();
    let mut version = None;
    let mut message = None;
    let mut updated_at = None;
    if let Some(raw) = raw.as_deref() {
        let mut parts = raw.trim().splitn(4, '|');
        state = parts.next().unwrap_or("unknown").to_string();
        version = parts
            .next()
            .filter(|v| !v.is_empty())
            .map(str::to_string);
        message = parts
            .next()
            .filter(|v| !v.is_empty())
            .map(str::to_string);
        updated_at = parts
            .next()
            .filter(|v| !v.is_empty())
            .map(str::to_string);
    }
    Ok(Json(UpdateStatusResponse {
        state,
        version,
        message,
        updated_at,
        updater_available: updater_available(),
        raw,
    }))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_update_states_block_a_second_request() {
        for state in [
            "queued",
            "pending",
            "downloading",
            "installing",
            "restarting",
            "running",
        ] {
            assert!(update_active(state), "{state}");
        }
        for state in ["idle", "success", "failed", "unknown"] {
            assert!(!update_active(state), "{state}");
        }
    }
}
