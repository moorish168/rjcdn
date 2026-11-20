//! Pull GeoLite2-Country from the control plane and hot-reload it.
//!
//! Triggered by:
//! - heartbeat `geoip_sha256` mismatch
//! - `POST /__lagran/geoip/reload` from control after an admin update
//! - a slow background poll (safety net)

use crate::agent::control_client::ControlClient;
use crate::agent::spawn_supervised;
use crate::config::EdgeConfig;
use crate::waf::geo::SharedGeo;
use bytes::Bytes;
use http::StatusCode;
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static RELOAD_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Wake the geoip sync loop (control notify or heartbeat mismatch).
pub fn request_geoip_reload() {
    RELOAD_REQUESTED.store(true, Ordering::Release);
}

fn take_reload_request() -> bool {
    RELOAD_REQUESTED.swap(false, Ordering::AcqRel)
}

#[derive(Debug, Deserialize)]
struct GeoIpMeta {
    #[serde(default)]
    sha256: String,
    #[serde(default)]
    size_bytes: u64,
    #[serde(default)]
    loaded: bool,
}

pub fn spawn_geoip_sync(config: EdgeConfig, control: ControlClient, geo: SharedGeo) {
    spawn_supervised("geoip_sync", move || {
        let config = config.clone();
        let control = control.clone();
        let geo = geo.clone();
        async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            loop {
                let force = take_reload_request();
                if let Err(e) = sync_once(&control, &geo, force).await {
                    tracing::warn!(error = %e, "geoip sync failed");
                }
                let wait = Duration::from_secs(config.heartbeat_interval_secs.max(30) * 4);
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {}
                    _ = async {
                        while !RELOAD_REQUESTED.load(Ordering::Acquire) {
                            tokio::time::sleep(Duration::from_millis(250)).await;
                        }
                    } => {}
                }
            }
        }
    });
}

async fn sync_once(control: &ControlClient, geo: &SharedGeo, force: bool) -> Result<(), String> {
    let meta: GeoIpMeta = control.get_json("/api/v1/geoip/meta").await?;
    if !meta.loaded || meta.sha256.is_empty() {
        return Ok(());
    }
    let local = geo.country_sha256();
    if !force && !local.is_empty() && local == meta.sha256 {
        return Ok(());
    }
    tracing::info!(
        remote = %meta.sha256,
        local = %local,
        size = meta.size_bytes,
        "pulling GeoLite2-Country from control"
    );
    let (sha, bytes) = download_country(control, &meta.sha256).await?;
    geo.replace_country(bytes.to_vec(), &sha)?;
    Ok(())
}

async fn download_country(
    control: &ControlClient,
    expected_sha: &str,
) -> Result<(String, Bytes), String> {
    let local = /* unused for If-None-Match when forcing */ expected_sha;
    let (status, headers, bytes) = control
        .get_bytes_conditional("/api/v1/geoip/country.mmdb", None)
        .await?;
    if status == StatusCode::NOT_MODIFIED {
        return Err("geoip country not modified unexpectedly".into());
    }
    if !status.is_success() {
        return Err(format!("HTTP {status} downloading geoip country"));
    }
    let sha = headers
        .get("x-lagran-geoip-sha256")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .or_else(|| {
            headers
                .get("etag")
                .and_then(|v| v.to_str().ok())
                .map(|e| e.trim_matches('"').to_string())
        })
        .unwrap_or_else(|| local.to_string());
    if bytes.len() < 1024 {
        return Err(format!("geoip country body too small ({} bytes)", bytes.len()));
    }
    Ok((sha, bytes))
}
