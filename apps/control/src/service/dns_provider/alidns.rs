//! Aliyun DNS (Alidns) OpenAPI for ACME TXT challenges.

use super::AliDnsCreds;
use chrono::Utc;
use ring::hmac;
use serde::Deserialize;
use std::collections::BTreeMap;
use uuid::Uuid;

fn percent_encode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() * 3);
    for &b in raw.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            b'*' => out.push_str("%2A"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn sign(secret: &str, string_to_sign: &str) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, format!("{secret}&").as_bytes());
    let tag = hmac::sign(&key, string_to_sign.as_bytes());
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, tag.as_ref())
}

async fn call(creds: &AliDnsCreds, mut params: BTreeMap<String, String>) -> Result<ValueBody, String> {
    params.insert("Format".into(), "JSON".into());
    params.insert("Version".into(), "2015-01-09".into());
    params.insert("AccessKeyId".into(), creds.access_key_id.clone());
    params.insert("SignatureMethod".into(), "HMAC-SHA1".into());
    params.insert("SignatureVersion".into(), "1.0".into());
    params.insert("SignatureNonce".into(), Uuid::new_v4().to_string());
    params.insert(
        "Timestamp".into(),
        Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    );

    let canonical = params
        .iter()
        .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let string_to_sign = format!(
        "GET&{}&{}",
        percent_encode("/"),
        percent_encode(&canonical)
    );
    let signature = sign(&creds.access_key_secret, &string_to_sign);
    let url = format!(
        "https://alidns.aliyuncs.com/?{canonical}&Signature={}",
        percent_encode(&signature)
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("alidns request: {e}"))?;
    let status = resp.status();
    let body: ValueBody = resp
        .json()
        .await
        .map_err(|e| format!("alidns json: {e}"))?;
    if !status.is_success()
        || body
            .code
            .as_deref()
            .is_some_and(|c| c != "OK" && !c.is_empty())
    {
        let msg = body
            .message
            .or(body.code)
            .unwrap_or_else(|| format!("HTTP {status}"));
        return Err(format!("alidns: {msg}"));
    }
    Ok(body)
}

#[derive(Debug, Deserialize)]
struct ValueBody {
    #[serde(default, rename = "Code")]
    code: Option<String>,
    #[serde(default, rename = "Message")]
    message: Option<String>,
    #[serde(default, rename = "DomainRecords")]
    domain_records: Option<DomainRecords>,
}

#[derive(Debug, Deserialize)]
struct DomainRecords {
    #[serde(default, rename = "Record")]
    record: Vec<Record>,
}

#[derive(Debug, Deserialize)]
struct Record {
    #[serde(default, rename = "RecordId")]
    record_id: String,
    #[serde(default, rename = "RR")]
    rr: String,
    #[serde(default, rename = "Type")]
    rtype: String,
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

async fn list_txt(creds: &AliDnsCreds, domain: &str, rr: &str) -> Result<Vec<String>, String> {
    let mut params = BTreeMap::new();
    params.insert("Action".into(), "DescribeDomainRecords".into());
    params.insert("DomainName".into(), domain.to_string());
    params.insert("RRKeyWord".into(), rr.to_string());
    params.insert("Type".into(), "TXT".into());
    params.insert("PageSize".into(), "100".into());
    let body = call(creds, params).await?;
    Ok(body
        .domain_records
        .map(|d| d.record)
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.rtype.eq_ignore_ascii_case("TXT") && r.rr.eq_ignore_ascii_case(rr))
        .map(|r| r.record_id)
        .collect())
}

pub async fn upsert_txt(creds: &AliDnsCreds, fqdn: &str, values: &[String]) -> Result<(), String> {
    let (domain, rr) = split_zone(fqdn)?;
    for id in list_txt(creds, &domain, &rr).await? {
        let mut params = BTreeMap::new();
        params.insert("Action".into(), "DeleteDomainRecord".into());
        params.insert("RecordId".into(), id);
        let _ = call(creds, params).await?;
    }
    for value in values {
        let mut params = BTreeMap::new();
        params.insert("Action".into(), "AddDomainRecord".into());
        params.insert("DomainName".into(), domain.clone());
        params.insert("RR".into(), rr.clone());
        params.insert("Type".into(), "TXT".into());
        params.insert("Value".into(), value.clone());
        params.insert("TTL".into(), "600".into());
        let _ = call(creds, params).await?;
    }
    Ok(())
}

pub async fn delete_txt(creds: &AliDnsCreds, fqdn: &str) -> Result<(), String> {
    let (domain, rr) = split_zone(fqdn)?;
    for id in list_txt(creds, &domain, &rr).await? {
        let mut params = BTreeMap::new();
        params.insert("Action".into(), "DeleteDomainRecord".into());
        params.insert("RecordId".into(), id);
        let _ = call(creds, params).await?;
    }
    Ok(())
}
