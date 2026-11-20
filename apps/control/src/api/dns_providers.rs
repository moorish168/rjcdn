//! Admin CRUD for customer DNS provider accounts (CNAME ACME DNS-01).

use crate::middleware::authz::Actor;
use crate::repository;
use crate::service::dns_provider;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

fn forbidden() -> (StatusCode, Json<Value>) {
    (StatusCode::FORBIDDEN, Json(json!({ "error": "forbidden" })))
}

fn bad_request(msg: impl Into<String>) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": msg.into() })))
}

fn internal(msg: impl Into<String>) -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": msg.into() })),
    )
}

#[derive(Serialize)]
pub struct ProviderDto {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub note: String,
    pub enabled: bool,
    pub credentials_configured: bool,
    pub created_at: String,
    pub updated_at: String,
}

fn to_dto(row: repository::DnsProviderRow) -> ProviderDto {
    ProviderDto {
        id: row.id.to_string(),
        name: row.name,
        provider_type: row.provider_type,
        note: row.note,
        enabled: row.enabled,
        credentials_configured: !row.credentials_enc.is_empty(),
        created_at: row.created_at.to_rfc3339(),
        updated_at: row.updated_at.to_rfc3339(),
    }
}

#[derive(Deserialize)]
pub struct CreateRequest {
    pub name: String,
    pub provider_type: String,
    pub credentials: Value,
    #[serde(default)]
    pub note: String,
}

#[derive(Deserialize)]
pub struct UpdateRequest {
    pub name: Option<String>,
    pub credentials: Option<Value>,
    pub note: Option<String>,
    pub enabled: Option<bool>,
}

pub async fn list(
    State(state): State<AppState>,
    actor: Actor,
) -> Result<Json<Vec<ProviderDto>>, (StatusCode, Json<Value>)> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let rows = repository::list_dns_providers(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "list dns providers");
            internal("database error")
        })?;
    Ok(Json(rows.into_iter().map(to_dto).collect()))
}

pub async fn create(
    State(state): State<AppState>,
    actor: Actor,
    Json(req): Json<CreateRequest>,
) -> Result<Json<ProviderDto>, (StatusCode, Json<Value>)> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(bad_request("name is required (max 64)"));
    }
    let provider_type = dns_provider::normalize_type(&req.provider_type).map_err(bad_request)?;
    let sealed = dns_provider::seal_credentials(
        &state.config.jwt_secret,
        provider_type,
        &req.credentials,
    )
    .map_err(bad_request)?;
    let id = repository::create_dns_provider(
        &state.pool,
        name,
        provider_type,
        &sealed,
        req.note.trim(),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "create dns provider");
        internal("database error")
    })?;
    let _ = repository::insert_audit(
        &state.pool,
        &actor.email,
        "dns_provider.create",
        &id.to_string(),
        &format!("type={provider_type}; name={name}"),
    )
    .await;
    let row = repository::get_dns_provider(&state.pool, id)
        .await
        .map_err(|_| internal("database error"))?
        .ok_or_else(|| internal("created provider missing"))?;
    Ok(Json(to_dto(row)))
}

pub async fn update(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateRequest>,
) -> Result<Json<ProviderDto>, (StatusCode, Json<Value>)> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let current = repository::get_dns_provider(&state.pool, id)
        .await
        .map_err(|_| internal("database error"))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))))?;
    let name = req
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&current.name);
    let note = req
        .note
        .as_deref()
        .map(str::trim)
        .unwrap_or(&current.note);
    let enabled = req.enabled.unwrap_or(current.enabled);
    let sealed = if let Some(creds) = req.credentials.as_ref() {
        Some(
            dns_provider::seal_credentials(
                &state.config.jwt_secret,
                &current.provider_type,
                creds,
            )
            .map_err(bad_request)?,
        )
    } else {
        None
    };
    let n = repository::update_dns_provider(
        &state.pool,
        id,
        name,
        sealed.as_deref(),
        note,
        enabled,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "update dns provider");
        internal("database error")
    })?;
    if n == 0 {
        return Err((StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))));
    }
    let _ = repository::insert_audit(
        &state.pool,
        &actor.email,
        "dns_provider.update",
        &id.to_string(),
        &format!("name={name}; enabled={enabled}"),
    )
    .await;
    let row = repository::get_dns_provider(&state.pool, id)
        .await
        .map_err(|_| internal("database error"))?
        .ok_or_else(|| internal("updated provider missing"))?;
    Ok(Json(to_dto(row)))
}

pub async fn delete(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let n = repository::delete_dns_provider(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "delete dns provider");
            internal("database error")
        })?;
    if n == 0 {
        return Err((StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))));
    }
    let _ = repository::insert_audit(
        &state.pool,
        &actor.email,
        "dns_provider.delete",
        &id.to_string(),
        "deleted",
    )
    .await;
    Ok(Json(json!({ "success": true })))
}
