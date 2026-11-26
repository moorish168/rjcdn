//! Visitor access logs (agent ingest + tenant-filtered read)

use crate::middleware::agent_auth::AgentAuth;
use crate::middleware::authz::Actor;
use crate::repository;
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

type ApiError = (StatusCode, Json<serde_json::Value>);
const MAX_PAGE_SIZE: i64 = 500;

fn db_error() -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": "database error" })),
    )
}

type AccessLogInsertRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    i32,
    i64,
    i32,
    String,
    String,
    String,
    String,
);

#[derive(Deserialize)]
pub struct AccessEntryIn {
    pub ts: Option<String>,
    pub node_id: Option<String>,
    pub client_ip: Option<String>,
    pub method: Option<String>,
    pub host: Option<String>,
    pub path: Option<String>,
    pub status: Option<u16>,
    pub bytes: Option<u64>,
    pub latency_ms: Option<u64>,
    pub cache_status: Option<String>,
    pub action: Option<String>,
    pub user_agent: Option<String>,
    pub referer: Option<String>,
}

#[derive(Deserialize)]
pub struct ReportBody {
    pub entries: Vec<AccessEntryIn>,
}

#[derive(Serialize)]
pub struct ReportAck {
    pub accepted: usize,
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub host: Option<String>,
    pub limit: Option<i64>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    /// 2xx / 3xx / 4xx / 5xx
    pub status_class: Option<String>,
}

#[derive(Serialize)]
pub struct AccessLogDto {
    pub id: i64,
    pub ts: String,
    pub node_id: String,
    pub client_ip: String,
    pub method: String,
    pub host: String,
    pub path: String,
    pub status: i32,
    pub bytes: i64,
    pub latency_ms: i32,
    pub cache_status: String,
    pub action: String,
    pub user_agent: String,
    pub referer: String,
}

#[derive(Serialize)]
pub struct AccessLogPageDto {
    pub items: Vec<AccessLogDto>,
    pub page: i64,
    pub page_size: i64,
    pub has_more: bool,
}

/// Split an ingest batch into rows that belong to a domain and hostless rows.
///
/// Keeping this pure lets the ingest boundary have a regression test without a
/// database: hostless rows are exactly what produced unattributed `—` entries
/// in the console.
fn partition_domain_rows(
    entries: Vec<AccessEntryIn>,
    node_name: &str,
) -> (Vec<AccessLogInsertRow>, usize, usize) {
    let mut rows = Vec::new();
    let mut rejected = 0usize;
    let mut bad_ts = 0usize;
    let now = chrono::Utc::now();
    for e in entries {
        let host = e.host.unwrap_or_default();
        if host.trim().is_empty() {
            rejected += 1;
            continue;
        }
        let Some(ts) = normalize_ts(e.ts.as_deref(), now) else {
            bad_ts += 1;
            continue;
        };
        rows.push((
            ts,
            // Attribution comes from the authenticated token, not the payload:
            // one agent must not be able to write entries for another node.
            node_name.to_string(),
            e.client_ip.unwrap_or_default(),
            e.method.unwrap_or_default(),
            host,
            e.path.unwrap_or_default(),
            e.status.unwrap_or(0) as i32,
            e.bytes.unwrap_or(0) as i64,
            e.latency_ms.unwrap_or(0) as i32,
            e.cache_status.unwrap_or_default(),
            e.action.unwrap_or_default(),
            e.user_agent.unwrap_or_default(),
            e.referer.unwrap_or_default(),
        ));
    }
    (rows, rejected, bad_ts)
}

/// Validate and clamp an agent-supplied timestamp. Bad values are dropped
/// instead of failing the whole batch inside Postgres (one bad row used to
/// reject every log from that node, forever, because the agent requeued it).
/// Exact host, or a one-label wildcard (`www.example.com` matches `*.example.com`).
fn host_logging_disabled(host: &str, disabled: &[String]) -> bool {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if disabled.iter().any(|name| name == &host) {
        return true;
    }
    let Some((_, rest)) = host.split_once('.') else {
        return false;
    };
    if rest.is_empty() {
        return false;
    }
    disabled.iter().any(|name| name == &format!("*.{rest}"))
}

fn normalize_ts(raw: Option<&str>, now: chrono::DateTime<chrono::Utc>) -> Option<String> {
    let raw = raw.unwrap_or("").trim();
    let parsed = if raw.is_empty() {
        now
    } else if let Ok(value) = chrono::DateTime::parse_from_rfc3339(raw) {
        value.with_timezone(&chrono::Utc)
    } else if let Ok(secs) = raw.parse::<i64>() {
        chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0)?
    } else {
        return None;
    };
    let max_ahead = now + chrono::Duration::minutes(5);
    let min_behind = now - chrono::Duration::days(1);
    let clamped = parsed.clamp(min_behind, max_ahead);
    Some(clamped.to_rfc3339())
}

pub async fn report(
    State(state): State<AppState>,
    agent: AgentAuth,
    Json(body): Json<ReportBody>,
) -> Json<ReportAck> {
    let (mut rows, rejected, bad_ts) = partition_domain_rows(body.entries, &agent.node_name);
    let dropped_sites = match repository::list_access_log_disabled_hosts(&state.pool).await {
        Ok(disabled) if !disabled.is_empty() => {
            let before = rows.len();
            rows.retain(|row| !host_logging_disabled(&row.4, &disabled));
            before - rows.len()
        }
        Ok(_) => 0,
        Err(e) => {
            tracing::warn!(error = %e, "access-log site filter unavailable; keeping the batch");
            0
        }
    };
    let n = rows.len();
    if rejected > 0 || bad_ts > 0 || dropped_sites > 0 {
        tracing::warn!(
            rejected,
            bad_ts,
            dropped_sites,
            accepted = n,
            "access-log report dropped invalid or disabled-site entries"
        );
    }
    if let Err(e) = repository::insert_access_logs_batch(&state.pool, &rows).await {
        tracing::error!(error = %e, "insert access logs");
        return Json(ReportAck { accepted: 0 });
    }
    Json(ReportAck { accepted: n })
}

pub async fn list(
    State(state): State<AppState>,
    actor: Actor,
    Query(q): Query<ListQuery>,
) -> Result<axum::response::Response, ApiError> {
    // Non-admins only see hosts that belong to their projects.
    let visible_hosts: Option<Vec<String>> = match actor.visible_domain_ids(&state).await {
        None => None,
        Some(ids) => Some(
            repository::hostnames_by_ids(&state.pool, &ids)
                .await
                .unwrap_or_default()
                .into_iter()
                .collect(),
        ),
    };

    if let Some(ref hosts) = visible_hosts {
        if let Some(ref h) = q.host {
            if !hosts.contains(h) {
                if q.page.is_some() || q.page_size.is_some() {
                    return Ok(Json(AccessLogPageDto {
                        items: Vec::new(),
                        page: q.page.unwrap_or(1).clamp(1, 1_000_000),
                        page_size: q
                            .page_size
                            .or(q.limit)
                            .unwrap_or(100)
                            .clamp(1, MAX_PAGE_SIZE),
                        has_more: false,
                    })
                    .into_response());
                }
                return Ok(Json(Vec::<AccessLogDto>::new()).into_response());
            }
        }
    }

    let (status_min, status_max) = match q.status_class.as_deref() {
        Some("2xx") => (Some(200), Some(300)),
        Some("3xx") => (Some(300), Some(400)),
        Some("4xx") => (Some(400), Some(500)),
        Some("5xx") => (Some(500), Some(600)),
        _ => (None, None),
    };
    let paginated = q.page.is_some() || q.page_size.is_some();
    let page = q.page.unwrap_or(1).clamp(1, 1_000_000);
    let page_size = q
        .page_size
        .unwrap_or_else(|| q.limit.unwrap_or(100))
        .clamp(1, MAX_PAGE_SIZE);
    let limit = if paginated {
        page_size + 1
    } else {
        q.limit.unwrap_or(100).clamp(1, MAX_PAGE_SIZE)
    };
    let offset = if paginated {
        page.saturating_sub(1).saturating_mul(page_size)
    } else {
        0
    };
    let rows = repository::list_access_logs(
        &state.pool,
        q.host.as_deref(),
        visible_hosts,
        status_min,
        status_max,
        limit,
        offset,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "list access logs");
        db_error()
    })?;
    let has_more = paginated && rows.len() as i64 > page_size;
    let mut rows = rows;
    if has_more {
        rows.truncate(page_size as usize);
    }
    let items: Vec<AccessLogDto> = rows
        .into_iter()
        .map(|r| AccessLogDto {
            id: r.id,
            ts: r.ts.to_rfc3339(),
            node_id: r.node_id,
            client_ip: r.client_ip,
            method: r.method,
            host: r.host,
            path: r.path,
            status: r.status,
            bytes: r.bytes,
            latency_ms: r.latency_ms,
            cache_status: r.cache_status,
            action: r.action,
            user_agent: r.user_agent,
            referer: r.referer,
        })
        .collect();
    if paginated {
        Ok(Json(AccessLogPageDto {
            items,
            page,
            page_size,
            has_more,
        })
        .into_response())
    } else {
        Ok(Json(items).into_response())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(host: Option<&str>, path: &str) -> AccessEntryIn {
        AccessEntryIn {
            ts: None,
            node_id: Some("edge-kr-01".into()),
            client_ip: Some("203.0.113.9".into()),
            method: Some("GET".into()),
            host: host.map(str::to_string),
            path: Some(path.into()),
            status: Some(404),
            bytes: Some(0),
            latency_ms: Some(0),
            cache_status: Some("NONE".into()),
            action: Some(String::new()),
            user_agent: Some("curl".into()),
            referer: None,
        }
    }

    #[test]
    fn hostless_ingest_rows_are_rejected() {
        let entries = vec![
            entry(None, "/.env-signed"),
            entry(Some("  "), "/mailjet.env"),
            entry(Some("cdn.example.com"), "/asset.js"),
        ];
        let (rows, rejected, bad_ts) = partition_domain_rows(entries, "edge-1");
        assert_eq!(rejected, 2);
        assert_eq!(bad_ts, 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, "edge-1");
        assert_eq!(rows[0].4, "cdn.example.com");
        assert_eq!(rows[0].5, "/asset.js");
    }
}
