//! Customer DNS provider APIs used for ACME DNS-01 under CNAME access.
//!
//! NS-mode sites write TXT into the platform hosted zone. CNAME-mode sites do
//! not control the customer's zone, so wildcard (and optional apex) issuance
//! needs the customer's DNS API credentials — the same model as GoEdge.

mod alidns;
mod cloudflare;
mod dnspod;

use crate::repository;
use crate::service::remote::crypto;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudflareCreds {
    pub api_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliDnsCreds {
    pub access_key_id: String,
    pub access_key_secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnspodCreds {
    /// `id,token` combined DNSPod token, or Tencent SecretId.
    pub secret_id: String,
    pub secret_key: String,
    /// `dnspod_token` | `tencent` — defaults to tencent cloud DNS API.
    #[serde(default)]
    pub api_type: String,
}

pub fn normalize_type(raw: &str) -> Result<&'static str, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "cloudflare" => Ok("cloudflare"),
        "alidns" | "aliyun" | "ali" => Ok("alidns"),
        "dnspod" | "tencent" | "tencentcloud" => Ok("dnspod"),
        other => Err(format!("unsupported DNS provider type: {other}")),
    }
}

pub fn seal_credentials(jwt_secret: &str, provider_type: &str, raw: &Value) -> Result<String, String> {
    let typed = normalize_type(provider_type)?;
    let json = match typed {
        "cloudflare" => {
            let token = raw
                .get("api_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if token.is_empty() {
                return Err("cloudflare api_token is required".into());
            }
            serde_json::to_string(&CloudflareCreds {
                api_token: token.to_string(),
            })
            .map_err(|e| e.to_string())?
        }
        "alidns" => {
            let id = raw
                .get("access_key_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let secret = raw
                .get("access_key_secret")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if id.is_empty() || secret.is_empty() {
                return Err("alidns access_key_id and access_key_secret are required".into());
            }
            serde_json::to_string(&AliDnsCreds {
                access_key_id: id.to_string(),
                access_key_secret: secret.to_string(),
            })
            .map_err(|e| e.to_string())?
        }
        "dnspod" => {
            let id = raw
                .get("secret_id")
                .or_else(|| raw.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let key = raw
                .get("secret_key")
                .or_else(|| raw.get("token"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if id.is_empty() || key.is_empty() {
                return Err("dnspod secret_id and secret_key are required".into());
            }
            let api_type = raw
                .get("api_type")
                .and_then(|v| v.as_str())
                .unwrap_or("tencent")
                .trim()
                .to_ascii_lowercase();
            serde_json::to_string(&DnspodCreds {
                secret_id: id.to_string(),
                secret_key: key.to_string(),
                api_type,
            })
            .map_err(|e| e.to_string())?
        }
        _ => return Err("unsupported DNS provider type".into()),
    };
    crypto::encrypt(jwt_secret, &json)
}

/// Upsert TXT values at `_acme-challenge.<host>` via the customer's DNS API.
pub async fn upsert_acme_txt(
    state: &AppState,
    provider_id: Uuid,
    host: &str,
    values: &[String],
) -> Result<(), String> {
    let provider = repository::get_dns_provider(&state.pool, provider_id)
        .await
        .map_err(|e| format!("load dns provider: {e}"))?
        .ok_or_else(|| "DNS 服务商账号不存在".to_string())?;
    if !provider.enabled {
        return Err("DNS 服务商账号已停用".into());
    }
    let plain = crypto::decrypt(&state.config.jwt_secret, &provider.credentials_enc)
        .map_err(|e| format!("decrypt dns provider credentials: {e}"))?;
    let fqdn = format!(
        "_acme-challenge.{}",
        host.trim().trim_end_matches('.').trim_start_matches("*.")
            .to_ascii_lowercase()
    );
    match provider.provider_type.as_str() {
        "cloudflare" => {
            let creds: CloudflareCreds =
                serde_json::from_str(&plain).map_err(|e| format!("cloudflare creds: {e}"))?;
            cloudflare::upsert_txt(&creds.api_token, &fqdn, values).await
        }
        "alidns" => {
            let creds: AliDnsCreds =
                serde_json::from_str(&plain).map_err(|e| format!("alidns creds: {e}"))?;
            alidns::upsert_txt(&creds, &fqdn, values).await
        }
        "dnspod" => {
            let creds: DnspodCreds =
                serde_json::from_str(&plain).map_err(|e| format!("dnspod creds: {e}"))?;
            dnspod::upsert_txt(&creds, &fqdn, values).await
        }
        other => Err(format!("unsupported DNS provider type: {other}")),
    }
}

pub async fn delete_acme_txt(
    state: &AppState,
    provider_id: Uuid,
    host: &str,
) -> Result<(), String> {
    let provider = repository::get_dns_provider(&state.pool, provider_id)
        .await
        .map_err(|e| format!("load dns provider: {e}"))?
        .ok_or_else(|| "DNS 服务商账号不存在".to_string())?;
    let plain = crypto::decrypt(&state.config.jwt_secret, &provider.credentials_enc)
        .map_err(|e| format!("decrypt dns provider credentials: {e}"))?;
    let fqdn = format!(
        "_acme-challenge.{}",
        host.trim().trim_end_matches('.').trim_start_matches("*.")
            .to_ascii_lowercase()
    );
    match provider.provider_type.as_str() {
        "cloudflare" => {
            let creds: CloudflareCreds =
                serde_json::from_str(&plain).map_err(|e| format!("cloudflare creds: {e}"))?;
            cloudflare::delete_txt(&creds.api_token, &fqdn).await
        }
        "alidns" => {
            let creds: AliDnsCreds =
                serde_json::from_str(&plain).map_err(|e| format!("alidns creds: {e}"))?;
            alidns::delete_txt(&creds, &fqdn).await
        }
        "dnspod" => {
            let creds: DnspodCreds =
                serde_json::from_str(&plain).map_err(|e| format!("dnspod creds: {e}"))?;
            dnspod::delete_txt(&creds, &fqdn).await
        }
        other => Err(format!("unsupported DNS provider type: {other}")),
    }
}
