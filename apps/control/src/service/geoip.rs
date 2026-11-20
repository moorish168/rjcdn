//! Download / reload MaxMind GeoLite2-Country for the control plane, and
//! expose the on-disk file for edge agents to pull.
//!
//! Default upstream (no MaxMind account):
//! `https://raw.githubusercontent.com/wp-statistics/GeoLite2-Country/master/GeoLite2-Country.mmdb.gz`

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use flate2::read::GzDecoder;
use serde::Serialize;

use crate::scheduler::GeoTable;

/// Community mirror of GeoLite2-Country (gzip). Used by the console「更新 IP 库」button.
pub const COUNTRY_MIRROR_GZ_URL: &str =
    "https://raw.githubusercontent.com/wp-statistics/GeoLite2-Country/master/GeoLite2-Country.mmdb.gz";

#[derive(Debug, Clone, Serialize)]
pub struct GeoIpStatus {
    pub path: String,
    pub loaded: bool,
    pub size_bytes: u64,
    pub sha256: String,
    pub updated_at: Option<String>,
    pub source: String,
}

/// Shared catalog: path + content hash that edges poll / receive via heartbeat.
#[derive(Clone)]
pub struct GeoIpCatalog {
    path: Arc<String>,
    inner: Arc<RwLock<CatalogInner>>,
}

#[derive(Debug, Default)]
struct CatalogInner {
    sha256: String,
    size_bytes: u64,
    updated_at: Option<DateTime<Utc>>,
    source: String,
    loaded: bool,
}

impl GeoIpCatalog {
    pub fn new(path: impl Into<String>) -> Self {
        let path = path.into();
        let catalog = Self {
            path: Arc::new(path.clone()),
            inner: Arc::new(RwLock::new(CatalogInner {
                source: "local".into(),
                ..Default::default()
            })),
        };
        catalog.refresh_from_disk();
        catalog
    }

    pub fn path(&self) -> &str {
        self.path.as_str()
    }

    pub fn sha256(&self) -> String {
        self.inner
            .read()
            .map(|g| g.sha256.clone())
            .unwrap_or_default()
    }

    pub fn status(&self) -> GeoIpStatus {
        let g = self.inner.read().unwrap_or_else(|e| e.into_inner());
        GeoIpStatus {
            path: self.path.as_str().into(),
            loaded: g.loaded,
            size_bytes: g.size_bytes,
            sha256: g.sha256.clone(),
            updated_at: g.updated_at.map(|t| t.to_rfc3339()),
            source: g.source.clone(),
        }
    }

    /// Re-read file metadata / hash after an external write or startup load.
    pub fn refresh_from_disk(&self) {
        let path = Path::new(self.path.as_str());
        let Ok(meta) = std::fs::metadata(path) else {
            if let Ok(mut g) = self.inner.write() {
                g.loaded = false;
                g.sha256.clear();
                g.size_bytes = 0;
            }
            return;
        };
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let sha = sha256_hex(&bytes);
        let mtime = meta.modified().ok().and_then(system_time_to_utc);
        if let Ok(mut g) = self.inner.write() {
            g.loaded = true;
            g.size_bytes = bytes.len() as u64;
            g.sha256 = sha;
            g.updated_at = mtime.or(g.updated_at);
            if g.source.is_empty() {
                g.source = "local".into();
            }
        }
    }

    pub fn mark_loaded(&self, sha256: String, size_bytes: u64, source: &str) {
        if let Ok(mut g) = self.inner.write() {
            g.loaded = true;
            g.sha256 = sha256;
            g.size_bytes = size_bytes;
            g.updated_at = Some(Utc::now());
            g.source = source.into();
        }
    }
}

fn system_time_to_utc(t: SystemTime) -> Option<DateTime<Utc>> {
    let dur = t.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    DateTime::from_timestamp(dur.as_secs() as i64, dur.subsec_nanos())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, bytes);
    hex_encode(digest.as_ref())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

/// True when the configured country DB is present and non-trivial on disk.
pub fn country_file_ready(path: &str) -> bool {
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.len() >= 1024)
        .unwrap_or(false)
}

/// Download the mirror gzip, validate as MMDB, atomically replace `path`, and
/// return a freshly loaded `GeoTable` plus the content hash.
pub async fn download_country_mirror(path: &str) -> anyhow::Result<(GeoTable, String, u64)> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("lagran-control/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let resp = client
        .get(COUNTRY_MIRROR_GZ_URL)
        .send()
        .await?
        .error_for_status()?;
    let gz_bytes = resp.bytes().await?;
    let mut decoder = GzDecoder::new(gz_bytes.as_ref());
    let mut mmdb = Vec::with_capacity(gz_bytes.len().saturating_mul(2));
    std::io::copy(&mut decoder, &mut mmdb)?;
    if mmdb.len() < 1024 {
        anyhow::bail!("downloaded GeoLite2-Country is too small ({} bytes)", mmdb.len());
    }
    // Validate before replacing the live file.
    let _ = maxminddb::Reader::from_source(mmdb.clone())
        .map_err(|e| anyhow::anyhow!("invalid MaxMind MMDB: {e}"))?;

    let dest = PathBuf::from(path);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = dest.with_extension("mmdb.tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&mmdb)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, &dest)?;
    let sha = sha256_hex(&mmdb);
    let size = mmdb.len() as u64;
    let table = GeoTable::load_with_ipv6(path, "");
    Ok((table, sha, size))
}

/// Read the country DB bytes for agent download (empty if missing).
pub fn read_country_file(path: &str) -> std::io::Result<Vec<u8>> {
    std::fs::read(path)
}
