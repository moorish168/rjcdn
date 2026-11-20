//! Cloudflare DNS API for ACME TXT challenges.

use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
struct CfListZones {
    success: bool,
    result: Vec<CfZone>,
    #[serde(default)]
    errors: Vec<CfError>,
}

#[derive(Debug, Deserialize)]
struct CfZone {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct CfListRecords {
    success: bool,
    result: Vec<CfRecord>,
    #[serde(default)]
    errors: Vec<CfError>,
}

#[derive(Debug, Deserialize)]
struct CfRecord {
    id: String,
}

#[derive(Debug, Deserialize)]
struct CfMutation {
    success: bool,
    #[serde(default)]
    errors: Vec<CfError>,
}

#[derive(Debug, Deserialize)]
struct CfError {
    message: String,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())
}

fn err_list(errors: &[CfError]) -> String {
    if errors.is_empty() {
        "Cloudflare API error".into()
    } else {
        errors
            .iter()
            .map(|e| e.message.as_str())
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// Split `_acme-challenge.www.example.com` → (zone `example.com`, name `_acme-challenge.www`).
async fn resolve_zone(
    client: &reqwest::Client,
    token: &str,
    fqdn: &str,
) -> Result<(String, String), String> {
    let host = fqdn.trim().trim_end_matches('.').to_ascii_lowercase();
    let labels: Vec<&str> = host.split('.').filter(|s| !s.is_empty()).collect();
    if labels.len() < 2 {
        return Err(format!("invalid challenge name {fqdn}"));
    }
    // Try longest public zones first by querying Cloudflare for candidate apexes.
    for i in 0..(labels.len() - 1) {
        let candidate = labels[i..].join(".");
        let url = format!("https://api.cloudflare.com/client/v4/zones?name={candidate}");
        let resp = client
            .get(&url)
            .bearer_auth(token)
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(|e| format!("cloudflare zones: {e}"))?;
        let body: CfListZones = resp
            .json()
            .await
            .map_err(|e| format!("cloudflare zones json: {e}"))?;
        if !body.success {
            return Err(err_list(&body.errors));
        }
        if let Some(zone) = body.result.into_iter().find(|z| z.name.eq_ignore_ascii_case(&candidate))
        {
            let relative = if host == zone.name {
                "@".to_string()
            } else if let Some(rest) = host.strip_suffix(&format!(".{}", zone.name)) {
                rest.to_string()
            } else {
                continue;
            };
            return Ok((zone.id, relative));
        }
    }
    Err(format!(
        "Cloudflare 账号下未找到覆盖 {host} 的 Zone；请确认域名已接入该账号"
    ))
}

async fn list_txt_ids(
    client: &reqwest::Client,
    token: &str,
    zone_id: &str,
    name: &str,
) -> Result<Vec<String>, String> {
    let url = format!(
        "https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records?type=TXT&name={name}"
    );
    let resp = client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("cloudflare list TXT: {e}"))?;
    let body: CfListRecords = resp
        .json()
        .await
        .map_err(|e| format!("cloudflare list TXT json: {e}"))?;
    if !body.success {
        return Err(err_list(&body.errors));
    }
    Ok(body.result.into_iter().map(|r| r.id).collect())
}

pub async fn upsert_txt(token: &str, fqdn: &str, values: &[String]) -> Result<(), String> {
    let client = client()?;
    let (zone_id, relative) = resolve_zone(&client, token, fqdn).await?;
    let record_name = if relative == "@" {
        // Cloudflare wants the FQDN for the name field.
        fqdn.trim().trim_end_matches('.').to_ascii_lowercase()
    } else {
        fqdn.trim().trim_end_matches('.').to_ascii_lowercase()
    };
    // Replace existing ACME TXT records so stale values don't confuse LE.
    for id in list_txt_ids(&client, token, &zone_id, &record_name).await? {
        let url = format!("https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records/{id}");
        let resp = client
            .delete(&url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("cloudflare delete TXT: {e}"))?;
        let body: CfMutation = resp
            .json()
            .await
            .map_err(|e| format!("cloudflare delete TXT json: {e}"))?;
        if !body.success {
            return Err(err_list(&body.errors));
        }
    }
    for value in values {
        let url = format!("https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records");
        let resp = client
            .post(&url)
            .bearer_auth(token)
            .json(&json!({
                "type": "TXT",
                "name": record_name,
                "content": value,
                "ttl": 120,
            }))
            .send()
            .await
            .map_err(|e| format!("cloudflare create TXT: {e}"))?;
        let body: CfMutation = resp
            .json()
            .await
            .map_err(|e| format!("cloudflare create TXT json: {e}"))?;
        if !body.success {
            return Err(err_list(&body.errors));
        }
    }
    Ok(())
}

pub async fn delete_txt(token: &str, fqdn: &str) -> Result<(), String> {
    let client = client()?;
    let (zone_id, _) = resolve_zone(&client, token, fqdn).await?;
    let record_name = fqdn.trim().trim_end_matches('.').to_ascii_lowercase();
    for id in list_txt_ids(&client, token, &zone_id, &record_name).await? {
        let url = format!("https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records/{id}");
        let resp = client
            .delete(&url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("cloudflare delete TXT: {e}"))?;
        let body: CfMutation = resp
            .json()
            .await
            .map_err(|e| format!("cloudflare delete TXT json: {e}"))?;
        if !body.success {
            return Err(err_list(&body.errors));
        }
    }
    Ok(())
}
