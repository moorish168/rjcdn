//! Tencent Cloud DNSPod API for ACME TXT challenges.

use super::DnspodCreds;
use ring::{digest, hmac};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
struct TcResponse {
    #[serde(default, rename = "Response")]
    response: Option<TcInner>,
}

#[derive(Debug, Deserialize)]
struct TcInner {
    #[serde(default, rename = "Error")]
    error: Option<TcError>,
    #[serde(default, rename = "RecordList")]
    record_list: Option<Vec<TcRecord>>,
}

#[derive(Debug, Deserialize)]
struct TcError {
    #[serde(default, rename = "Message")]
    message: String,
    #[serde(default, rename = "Code")]
    code: String,
}

#[derive(Debug, Deserialize)]
struct TcRecord {
    #[serde(default, rename = "RecordId")]
    record_id: u64,
    #[serde(default, rename = "Name")]
    name: String,
    #[serde(default, rename = "Type")]
    rtype: String,
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn split_zone(fqdn: &str) -> Result<(String, String), String> {
    let host = fqdn.trim().trim_end_matches('.').to_ascii_lowercase();
    let labels: Vec<&str> = host.split('.').filter(|s| !s.is_empty()).collect();
    if labels.len() < 2 {
        return Err(format!("invalid challenge name {fqdn}"));
    }
    let (domain, rr) = if labels.len() >= 3
        && matches!(
            labels[labels.len() - 2],
            "com" | "net" | "org" | "gov" | "edu" | "co"
        )
    {
        let domain = labels[labels.len() - 3..].join(".");
        let rr = labels[..labels.len() - 3].join(".");
        (domain, if rr.is_empty() { "@".into() } else { rr })
    } else {
        let domain = labels[labels.len() - 2..].join(".");
        let rr = labels[..labels.len() - 2].join(".");
        (domain, if rr.is_empty() { "@".into() } else { rr })
    };
    Ok((domain, rr))
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let key = hmac::Key::new(hmac::HMAC_SHA256, key);
    hmac::sign(&key, data).as_ref().to_vec()
}

async fn tc3_post(creds: &DnspodCreds, action: &str, payload: Value) -> Result<TcInner, String> {
    let service = "dnspod";
    let host = "dnspod.tencentcloudapi.com";
    let version = "2021-03-23";
    let timestamp = chrono::Utc::now().timestamp();
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let payload_str = payload.to_string();
    let hashed_payload = hex_encode(digest::digest(&digest::SHA256, payload_str.as_bytes()).as_ref());

    let canonical_headers = format!("content-type:application/json; charset=utf-8\nhost:{host}\n");
    let signed_headers = "content-type;host";
    let canonical_request = format!(
        "POST\n/\n\n{canonical_headers}\n{signed_headers}\n{hashed_payload}"
    );
    let hashed_canonical =
        hex_encode(digest::digest(&digest::SHA256, canonical_request.as_bytes()).as_ref());
    let credential_scope = format!("{date}/{service}/tc3_request");
    let string_to_sign =
        format!("TC3-HMAC-SHA256\n{timestamp}\n{credential_scope}\n{hashed_canonical}");

    let secret_date = hmac_sha256(format!("TC3{}", creds.secret_key).as_bytes(), date.as_bytes());
    let secret_service = hmac_sha256(&secret_date, service.as_bytes());
    let secret_signing = hmac_sha256(&secret_service, b"tc3_request");
    let signature = hex_encode(&hmac_sha256(&secret_signing, string_to_sign.as_bytes()));

    let authorization = format!(
        "TC3-HMAC-SHA256 Credential={}/{}, SignedHeaders={signed_headers}, Signature={signature}",
        creds.secret_id, credential_scope
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(format!("https://{host}"))
        .header("Authorization", authorization)
        .header("Content-Type", "application/json; charset=utf-8")
        .header("Host", host)
        .header("X-TC-Action", action)
        .header("X-TC-Timestamp", timestamp.to_string())
        .header("X-TC-Version", version)
        .body(payload_str)
        .send()
        .await
        .map_err(|e| format!("dnspod request: {e}"))?;
    let body: TcResponse = resp
        .json()
        .await
        .map_err(|e| format!("dnspod json: {e}"))?;
    let inner = body
        .response
        .ok_or_else(|| "dnspod empty response".to_string())?;
    if let Some(err) = inner.error {
        return Err(format!("dnspod {}: {}", err.code, err.message));
    }
    Ok(inner)
}

async fn legacy_token_post(
    creds: &DnspodCreds,
    action: &str,
    extra: &[(&str, String)],
) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let mut form = vec![
        (
            "login_token",
            format!("{},{}", creds.secret_id, creds.secret_key),
        ),
        ("format", "json".into()),
    ];
    for (k, v) in extra {
        form.push((k, v.clone()));
    }
    let url = format!("https://dnsapi.cn/{action}");
    let resp = client
        .post(&url)
        .form(&form)
        .send()
        .await
        .map_err(|e| format!("dnspod legacy: {e}"))?;
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("dnspod legacy json: {e}"))?;
    let status = body
        .pointer("/status/code")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if status != "1" {
        let msg = body
            .pointer("/status/message")
            .and_then(|v| v.as_str())
            .unwrap_or("error");
        return Err(format!("dnspod: {msg}"));
    }
    Ok(body)
}

pub async fn upsert_txt(creds: &DnspodCreds, fqdn: &str, values: &[String]) -> Result<(), String> {
    let (domain, rr) = split_zone(fqdn)?;
    if creds.api_type == "dnspod_token" {
        if let Ok(body) = legacy_token_post(
            creds,
            "Record.List",
            &[
                ("domain", domain.clone()),
                ("sub_domain", rr.clone()),
                ("record_type", "TXT".into()),
            ],
        )
        .await
        {
            if let Some(arr) = body.get("records").and_then(|v| v.as_array()) {
                for rec in arr {
                    if let Some(id) = rec.get("id").and_then(|v| v.as_str()) {
                        let _ = legacy_token_post(
                            creds,
                            "Record.Remove",
                            &[("domain", domain.clone()), ("record_id", id.to_string())],
                        )
                        .await;
                    }
                }
            }
        }
        for value in values {
            let _ = legacy_token_post(
                creds,
                "Record.Create",
                &[
                    ("domain", domain.clone()),
                    ("sub_domain", rr.clone()),
                    ("record_type", "TXT".into()),
                    ("record_line", "默认".into()),
                    ("value", value.clone()),
                    ("ttl", "600".into()),
                ],
            )
            .await?;
        }
        return Ok(());
    }

    if let Ok(inner) = tc3_post(
        creds,
        "DescribeRecordList",
        json!({
            "Domain": domain,
            "Subdomain": rr,
            "RecordType": "TXT",
            "Limit": 100,
        }),
    )
    .await
    {
        for rec in inner.record_list.unwrap_or_default() {
            if rec.rtype.eq_ignore_ascii_case("TXT") {
                let _ = tc3_post(
                    creds,
                    "DeleteRecord",
                    json!({ "Domain": domain, "RecordId": rec.record_id }),
                )
                .await;
            }
        }
    }
    for value in values {
        let _ = tc3_post(
            creds,
            "CreateRecord",
            json!({
                "Domain": domain,
                "SubDomain": rr,
                "RecordType": "TXT",
                "RecordLine": "默认",
                "Value": value,
                "TTL": 600,
            }),
        )
        .await?;
    }
    Ok(())
}

pub async fn delete_txt(creds: &DnspodCreds, fqdn: &str) -> Result<(), String> {
    let (domain, rr) = split_zone(fqdn)?;
    if creds.api_type == "dnspod_token" {
        let body = legacy_token_post(
            creds,
            "Record.List",
            &[
                ("domain", domain.clone()),
                ("sub_domain", rr.clone()),
                ("record_type", "TXT".into()),
            ],
        )
        .await?;
        if let Some(arr) = body.get("records").and_then(|v| v.as_array()) {
            for rec in arr {
                if let Some(id) = rec.get("id").and_then(|v| v.as_str()) {
                    let _ = legacy_token_post(
                        creds,
                        "Record.Remove",
                        &[("domain", domain.clone()), ("record_id", id.to_string())],
                    )
                    .await?;
                }
            }
        }
        return Ok(());
    }
    let inner = tc3_post(
        creds,
        "DescribeRecordList",
        json!({
            "Domain": domain,
            "Subdomain": rr,
            "RecordType": "TXT",
            "Limit": 100,
        }),
    )
    .await?;
    for rec in inner.record_list.unwrap_or_default() {
        if rec.rtype.eq_ignore_ascii_case("TXT") || rec.name.eq_ignore_ascii_case(&rr) {
            let _ = tc3_post(
                creds,
                "DeleteRecord",
                json!({ "Domain": domain, "RecordId": rec.record_id }),
            )
            .await?;
        }
    }
    Ok(())
}
