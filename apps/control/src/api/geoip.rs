//! Admin GeoIP update + agent download of GeoLite2-Country.

use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use serde_json::{json, Value};

use crate::middleware::agent_auth::AgentAuth;
use crate::middleware::authz::Actor;
use crate::service::geoip::{self, GeoIpStatus};
use crate::state::AppState;

fn forbidden() -> (StatusCode, Json<Value>) {
    (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})))
}

fn internal(message: impl Into<String>) -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": message.into()})),
    )
}

#[derive(Serialize)]
pub struct GeoIpStatusResponse {
    #[serde(flatten)]
    pub status: GeoIpStatus,
    pub mirror_url: String,
}

/// Console: current Country DB status.
pub async fn status(
    State(state): State<AppState>,
    actor: Actor,
) -> Result<Json<GeoIpStatusResponse>, (StatusCode, Json<Value>)> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    Ok(Json(GeoIpStatusResponse {
        status: state.geoip.status(),
        mirror_url: geoip::COUNTRY_MIRROR_GZ_URL.into(),
    }))
}

/// Console: download mirror → hot-reload control → notify edges to pull.
pub async fn update(
    State(state): State<AppState>,
    actor: Actor,
) -> Result<Json<GeoIpStatusResponse>, (StatusCode, Json<Value>)> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let path = state.geoip.path().to_string();
    let (table, sha, size) = geoip::download_country_mirror(&path)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "geoip country update failed");
            internal(format!("更新 IP 库失败: {e}"))
        })?;
    state.replace_geo(table);
    state
        .geoip
        .mark_loaded(sha, size, "wp-statistics/GeoLite2-Country");
    // Best-effort: wake edges so they pull without waiting for the next heartbeat.
    crate::service::edge_notify::notify_geoip_reload(&state).await;
    Ok(Json(GeoIpStatusResponse {
        status: state.geoip.status(),
        mirror_url: geoip::COUNTRY_MIRROR_GZ_URL.into(),
    }))
}

/// Edge agent: download the current Country MMDB (ETag = sha256).
pub async fn agent_country(
    State(state): State<AppState>,
    _agent: AgentAuth,
    headers: HeaderMap,
) -> Result<Response, (StatusCode, Json<Value>)> {
    let status = state.geoip.status();
    if status.sha256.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "geoip country database not available on control"})),
        ));
    }
    let etag = format!("\"{}\"", status.sha256);
    if let Some(inm) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
    {
        if inm.split(',').any(|t| t.trim() == etag || t.trim() == status.sha256) {
            return Ok(StatusCode::NOT_MODIFIED.into_response());
        }
    }
    let bytes = geoip::read_country_file(state.geoip.path()).map_err(|e| {
        tracing::error!(error = %e, "read geoip country for agent");
        internal("cannot read geoip country database")
    })?;
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = StatusCode::OK;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Ok(v) = HeaderValue::from_str(&etag) {
        headers.insert(header::ETAG, v);
    }
    if let Ok(v) = HeaderValue::from_str(&status.sha256) {
        headers.insert(
            header::HeaderName::from_static("x-lagran-geoip-sha256"),
            v,
        );
    }
    Ok(response)
}

/// Edge agent: lightweight meta (sha + size) without downloading the blob.
pub async fn agent_meta(
    State(state): State<AppState>,
    _agent: AgentAuth,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let s = state.geoip.status();
    Ok(Json(json!({
        "sha256": s.sha256,
        "size_bytes": s.size_bytes,
        "updated_at": s.updated_at,
        "loaded": s.loaded,
    })))
}
