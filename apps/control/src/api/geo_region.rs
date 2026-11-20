//! Smart DNS lines, plus the continent → country catalog the console pickers
//! (DNS lines, WAF 地区名单) share.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::middleware::authz::Actor;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: String,
}

type ApiError = (StatusCode, Json<ErrorBody>);

fn bad_request(error: impl Into<String>) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorBody {
            error: error.into(),
        }),
    )
}

fn forbidden() -> ApiError {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorBody {
            error: "admin only".into(),
        }),
    )
}

fn db_error() -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorBody {
            error: "database error".into(),
        }),
    )
}

fn not_found() -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorBody {
            error: "not found".into(),
        }),
    )
}

const REGION_COLS: &str = r#"
    id, code, name, kind, country, province, city, isp, continent, countries,
    is_default, enabled, sort_order, note
"#;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct GeoRegionDto {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub kind: String,
    pub country: String,
    pub province: String,
    pub city: String,
    pub isp: String,
    /// MaxMind continent code (NA/EU/…) when the line covers a whole continent.
    #[serde(default)]
    pub continent: String,
    /// Comma-separated ISO country codes (US,CA,…). Preferred match set.
    #[serde(default)]
    pub countries: String,
    pub is_default: bool,
    pub enabled: bool,
    pub sort_order: i32,
    pub note: String,
}

#[derive(Debug, Deserialize)]
pub struct UpsertRegionRequest {
    pub code: Option<String>,
    pub name: Option<String>,
    pub kind: Option<String>,
    pub country: Option<String>,
    pub province: Option<String>,
    pub city: Option<String>,
    pub isp: Option<String>,
    pub continent: Option<String>,
    /// Accept either a CSV string or a JSON array of ISO codes.
    #[serde(default)]
    pub countries: Option<CountriesInput>,
    pub is_default: Option<bool>,
    pub enabled: Option<bool>,
    pub sort_order: Option<i32>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum CountriesInput {
    Csv(String),
    List(Vec<String>),
}

impl CountriesInput {
    fn normalized(&self) -> String {
        match self {
            CountriesInput::Csv(s) => normalize_countries_csv(s),
            CountriesInput::List(list) => normalize_countries_list(list),
        }
    }
}

fn normalize_countries_csv(raw: &str) -> String {
    normalize_countries_list(
        &raw.split(|c: char| c == ',' || c == ';' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
    )
}

fn normalize_countries_list(list: &[String]) -> String {
    let mut out: Vec<String> = list
        .iter()
        .map(|s| s.trim().to_ascii_uppercase())
        .filter(|s| s.len() == 2 && s.chars().all(|c| c.is_ascii_alphabetic()))
        .collect();
    out.sort();
    out.dedup();
    out.join(",")
}

/// Parse stored `countries` CSV into uppercase ISO codes.
pub fn countries_list(region: &GeoRegionDto) -> Vec<String> {
    if region.countries.trim().is_empty() {
        return Vec::new();
    }
    region
        .countries
        .split(',')
        .map(|s| s.trim().to_ascii_uppercase())
        .filter(|s| !s.is_empty())
        .collect()
}

#[derive(Debug, Serialize)]
pub struct CountryCatalogOut {
    pub code: String,
    pub name_zh: String,
    pub name_en: String,
}

#[derive(Debug, Serialize)]
pub struct ContinentCatalogOut {
    pub code: String,
    pub name_zh: String,
    pub name_en: String,
    pub countries: Vec<CountryCatalogOut>,
}

/// Static continent → country tree for console pickers (not from MaxMind).
pub async fn catalog(_actor: Actor) -> Json<Vec<ContinentCatalogOut>> {
    Json(
        crate::geo_catalog::CONTINENTS
            .iter()
            .map(|c| ContinentCatalogOut {
                code: c.code.into(),
                name_zh: c.name_zh.into(),
                name_en: c.name_en.into(),
                countries: c
                    .countries
                    .iter()
                    .map(|co| CountryCatalogOut {
                        code: co.code.into(),
                        name_zh: co.name_zh.into(),
                        name_en: co.name_en.into(),
                    })
                    .collect(),
            })
            .collect(),
    )
}

pub async fn list(
    State(state): State<AppState>,
    _actor: Actor,
) -> Result<Json<Vec<GeoRegionDto>>, ApiError> {
    let rows = sqlx::query_as::<_, GeoRegionDto>(&format!(
        "SELECT {REGION_COLS} FROM geo_regions ORDER BY sort_order, name"
    ))
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "list geo_regions");
        db_error()
    })?;
    Ok(Json(rows))
}

pub async fn create(
    State(state): State<AppState>,
    actor: Actor,
    Json(req): Json<UpsertRegionRequest>,
) -> Result<(StatusCode, Json<GeoRegionDto>), ApiError> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let code = req
        .code
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad_request("code required"))?
        .to_ascii_lowercase();
    let name = req
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad_request("name required"))?
        .to_string();
    let kind = normalize_kind(req.kind.as_deref().unwrap_or("custom"))?;
    let is_default = req.is_default.unwrap_or(false);
    if is_default {
        clear_default(&state).await?;
    }
    let continent = normalize_continent(req.continent.as_deref().unwrap_or(""));
    let countries = req
        .countries
        .as_ref()
        .map(CountriesInput::normalized)
        .unwrap_or_default();
    let country = req.country.as_deref().unwrap_or("").trim().to_ascii_uppercase();
    let row = sqlx::query_as::<_, GeoRegionDto>(&format!(
        r#"
        INSERT INTO geo_regions
            (code, name, kind, country, province, city, isp, continent, countries,
             is_default, enabled, sort_order, note)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
        RETURNING {REGION_COLS}
        "#
    ))
    .bind(&code)
    .bind(&name)
    .bind(&kind)
    .bind(&country)
    .bind(req.province.as_deref().unwrap_or("").trim())
    .bind(req.city.as_deref().unwrap_or("").trim())
    .bind(req.isp.as_deref().unwrap_or("").trim())
    .bind(&continent)
    .bind(&countries)
    .bind(is_default)
    .bind(req.enabled.unwrap_or(true))
    .bind(req.sort_order.unwrap_or(100))
    .bind(req.note.as_deref().unwrap_or("").trim())
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if e.as_database_error()
            .is_some_and(|d| d.is_unique_violation())
        {
            return bad_request("code already exists");
        }
        tracing::error!(error = %e, "create geo_region");
        db_error()
    })?;
    invalidate_cache();
    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn update(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
    Json(req): Json<UpsertRegionRequest>,
) -> Result<Json<GeoRegionDto>, ApiError> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let uid = Uuid::parse_str(&id).map_err(|_| bad_request("invalid id"))?;
    let current = sqlx::query_as::<_, GeoRegionDto>(&format!(
        "SELECT {REGION_COLS} FROM geo_regions WHERE id = $1"
    ))
    .bind(uid)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| db_error())?
    .ok_or_else(not_found)?;

    let code = req
        .code
        .as_deref()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| current.code.clone());
    let name = req
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&current.name)
        .to_string();
    let kind = match req.kind.as_deref() {
        Some(k) => normalize_kind(k)?,
        None => current.kind.clone(),
    };
    let is_default = req.is_default.unwrap_or(current.is_default);
    let continent = match req.continent.as_deref() {
        Some(c) => normalize_continent(c),
        None => current.continent.clone(),
    };
    let countries = match req.countries.as_ref() {
        Some(c) => c.normalized(),
        None => current.countries.clone(),
    };
    let country = match req.country.as_deref() {
        Some(c) => c.trim().to_ascii_uppercase(),
        None => current.country.clone(),
    };
    let enabled = req.enabled.unwrap_or(current.enabled);
    let dns_refs = region_usage(&state.pool, &current)
        .await
        .map_err(|_| db_error())?;
    if dns_refs > 0 && !enabled && current.enabled {
        return Err(bad_request(format!(
            "该线路仍被 {dns_refs} 条 DNS 记录使用，不能停用"
        )));
    }
    if is_default && !current.is_default {
        clear_default(&state).await?;
    }
    if !is_default && current.is_default {
        return Err(bad_request(
            "cannot unset the only default line; set another region as default first",
        ));
    }

    let row = sqlx::query_as::<_, GeoRegionDto>(&format!(
        r#"
        UPDATE geo_regions SET
            code = $2, name = $3, kind = $4,
            country = $5, province = $6, city = $7, isp = $8,
            continent = $9, countries = $10,
            is_default = $11, enabled = $12, sort_order = $13, note = $14,
            updated_at = NOW()
        WHERE id = $1
        RETURNING {REGION_COLS}
        "#
    ))
    .bind(uid)
    .bind(&code)
    .bind(&name)
    .bind(&kind)
    .bind(&country)
    .bind(
        req.province
            .as_deref()
            .map(str::trim)
            .unwrap_or(&current.province),
    )
    .bind(req.city.as_deref().map(str::trim).unwrap_or(&current.city))
    .bind(req.isp.as_deref().map(str::trim).unwrap_or(&current.isp))
    .bind(&continent)
    .bind(&countries)
    .bind(is_default)
    .bind(req.enabled.unwrap_or(current.enabled))
    .bind(req.sort_order.unwrap_or(current.sort_order))
    .bind(req.note.as_deref().map(str::trim).unwrap_or(&current.note))
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if e.as_database_error()
            .is_some_and(|d| d.is_unique_violation())
        {
            return bad_request("code already exists or default conflict");
        }
        tracing::error!(error = %e, "update geo_region");
        db_error()
    })?;
    invalidate_cache();
    Ok(Json(row))
}

pub async fn delete(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !actor.is_admin {
        return Err(forbidden());
    }
    let uid = Uuid::parse_str(&id).map_err(|_| bad_request("invalid id"))?;
    let is_default: bool = sqlx::query_scalar("SELECT is_default FROM geo_regions WHERE id = $1")
        .bind(uid)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| db_error())?
        .ok_or_else(not_found)?;
    if is_default {
        return Err(bad_request("cannot delete the default line"));
    }
    if let Some(region) = sqlx::query_as::<_, GeoRegionDto>(&format!(
        "SELECT {REGION_COLS} FROM geo_regions WHERE id = $1"
    ))
    .bind(uid)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| db_error())?
    {
        let dns_refs = region_usage(&state.pool, &region)
            .await
            .map_err(|_| db_error())?;
        if dns_refs > 0 {
            return Err(bad_request(format!(
                "该线路仍被 {dns_refs} 条 DNS 记录使用，请先修改这些记录"
            )));
        }
    }
    let r = sqlx::query("DELETE FROM geo_regions WHERE id = $1")
        .bind(uid)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "delete geo_region");
            db_error()
        })?;
    if r.rows_affected() == 0 {
        return Err(not_found());
    }
    invalidate_cache();
    Ok(StatusCode::NO_CONTENT)
}

async fn clear_default(state: &AppState) -> Result<(), ApiError> {
    sqlx::query("UPDATE geo_regions SET is_default = FALSE WHERE is_default = TRUE")
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "clear default geo_region");
            db_error()
        })?;
    Ok(())
}

fn normalize_kind(kind: &str) -> Result<String, ApiError> {
    let k = kind.trim().to_ascii_lowercase();
    match k.as_str() {
        "default" | "country" | "continent" | "province" | "isp" | "custom" => Ok(k),
        _ => Err(bad_request(
            "kind must be default|country|continent|province|isp|custom",
        )),
    }
}

fn normalize_continent(raw: &str) -> String {
    let c = raw.trim().to_ascii_uppercase();
    match c.as_str() {
        "AF" | "AN" | "AS" | "EU" | "NA" | "OC" | "SA" => c,
        "" => String::new(),
        _ => c.chars().take(2).collect(),
    }
}

/// Match client geo against a region row (shared by DNS scheduler + WAF expand).
pub fn region_matches(
    region: &GeoRegionDto,
    continent: &str,
    country: &str,
    province: &str,
    city: &str,
    isp: &str,
) -> bool {
    if !region.enabled {
        return false;
    }
    if region.is_default || region.kind == "default" {
        return true;
    }
    let eq = |a: &str, b: &str| !a.is_empty() && a.eq_ignore_ascii_case(b);
    let countries = countries_list(region);

    // Multi-country / continent set (preferred for 洲→国家勾选).
    if !countries.is_empty() {
        if !countries.iter().any(|c| eq(c, country)) {
            return false;
        }
    } else if !region.continent.is_empty() {
        let client_cont = if continent.is_empty() {
            crate::geo_catalog::continent_for_country(country).unwrap_or("")
        } else {
            continent
        };
        if !eq(&region.continent, client_cont) {
            return false;
        }
    } else if !region.country.is_empty() && !eq(&region.country, country) {
        return false;
    }

    if !region.province.is_empty() && !eq(&region.province, province) {
        return false;
    }
    if !region.city.is_empty() && !eq(&region.city, city) {
        return false;
    }
    if !region.isp.is_empty() && !eq(&region.isp, isp) {
        return false;
    }
    // At least one field must have constrained the match.
    if countries.is_empty()
        && region.continent.is_empty()
        && region.country.is_empty()
        && region.province.is_empty()
        && region.city.is_empty()
        && region.isp.is_empty()
    {
        if region.code.eq_ignore_ascii_case("overseas") {
            return !country.eq_ignore_ascii_case("CN") && !country.is_empty();
        }
        return false;
    }
    true
}

/// How many DNS records answer on a line. WAF rules pick continents and
/// countries directly and never reference a line.
async fn region_usage(pool: &sqlx::PgPool, region: &GeoRegionDto) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM dns_records WHERE line_id = $1")
        .bind(region.id)
        .fetch_one(pool)
        .await
}

type RegionCache = tokio::sync::Mutex<Option<(std::time::Instant, std::sync::Arc<Vec<GeoRegionDto>>)>>;

fn region_cache() -> &'static RegionCache {
    static CACHE: std::sync::OnceLock<RegionCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| tokio::sync::Mutex::new(None))
}

/// Drop the cached catalog after a change.
pub fn invalidate_cache() {
    if let Ok(mut guard) = region_cache().try_lock() {
        *guard = None;
    } else {
        let cache = region_cache();
        tokio::spawn(async move {
            *cache.lock().await = None;
        });
    }
}

/// Enabled regions, cached for a few seconds. The scheduler runs on every DNS
/// cache miss and must not query the catalog each time.
pub async fn load_cached(pool: &sqlx::PgPool) -> sqlx::Result<std::sync::Arc<Vec<GeoRegionDto>>> {
    const TTL: std::time::Duration = std::time::Duration::from_secs(15);
    let mut guard = region_cache().lock().await;
    if let Some((at, regions)) = guard.as_ref() {
        if at.elapsed() < TTL {
            return Ok(regions.clone());
        }
    }
    let regions = std::sync::Arc::new(load_all(pool).await?);
    *guard = Some((std::time::Instant::now(), regions.clone()));
    Ok(regions)
}

pub async fn load_all(pool: &sqlx::PgPool) -> sqlx::Result<Vec<GeoRegionDto>> {
    sqlx::query_as::<_, GeoRegionDto>(&format!(
        "SELECT {REGION_COLS} FROM geo_regions WHERE enabled = TRUE ORDER BY sort_order, name"
    ))
    .fetch_all(pool)
    .await
}

pub async fn default_region(pool: &sqlx::PgPool) -> sqlx::Result<Option<GeoRegionDto>> {
    sqlx::query_as::<_, GeoRegionDto>(&format!(
        "SELECT {REGION_COLS} FROM geo_regions WHERE is_default = TRUE LIMIT 1"
    ))
    .fetch_optional(pool)
    .await
}

/// Pick the best matching non-default region, else the default line.
///
/// Custom lines outrank each other by specificity; anything unmatched
/// (including GeoIP miss) falls through to the default line.
pub fn pick_region<'a>(
    regions: &'a [GeoRegionDto],
    continent: &str,
    country: &str,
    province: &str,
    city: &str,
    isp: &str,
) -> Option<&'a GeoRegionDto> {
    let mut best: Option<&GeoRegionDto> = None;
    let mut best_score = -1i32;
    for r in regions {
        if r.is_default || r.kind == "default" {
            continue;
        }
        if !region_matches(r, continent, country, province, city, isp) {
            continue;
        }
        let mut score = 0;
        // Prefer explicit custom lines over any leftover kind tags.
        if r.kind.eq_ignore_ascii_case("custom") {
            score += 1_000;
        }
        if !r.isp.is_empty() {
            score += 40;
        }
        if !r.province.is_empty() {
            score += 30;
        }
        if !r.city.is_empty() {
            score += 20;
        }
        let countries = countries_list(r);
        if countries.len() == 1 || (!r.country.is_empty() && countries.is_empty() && r.continent.is_empty())
        {
            score += 10;
        } else if countries.len() > 1 {
            score += 8;
        } else if !r.continent.is_empty() {
            score += 3;
        }
        if score > best_score {
            best_score = score;
            best = Some(r);
        }
    }
    best.or_else(|| regions.iter().find(|r| r.is_default))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn region(
        code: &str,
        kind: &str,
        country: &str,
        province: &str,
        isp: &str,
        continent: &str,
        countries: &str,
        is_default: bool,
    ) -> GeoRegionDto {
        GeoRegionDto {
            id: Uuid::new_v4(),
            code: code.into(),
            name: code.into(),
            kind: kind.into(),
            country: country.into(),
            province: province.into(),
            city: String::new(),
            isp: isp.into(),
            continent: continent.into(),
            countries: countries.into(),
            is_default,
            enabled: true,
            sort_order: 0,
            note: String::new(),
        }
    }

    #[test]
    fn custom_line_beats_standard_isp_and_province() {
        let regions = vec![
            region("default", "default", "", "", "", "", "", true),
            region("cn-telecom", "isp", "CN", "", "电信", "", "", false),
            region("cn-gd", "province", "CN", "广东", "", "", "", false),
            region("mine-cn", "custom", "CN", "", "", "", "CN", false),
        ];
        let picked = pick_region(&regions, "AS", "CN", "广东", "", "电信").unwrap();
        assert_eq!(picked.code, "mine-cn");
    }

    #[test]
    fn among_customs_more_specific_wins() {
        let regions = vec![
            region("default", "default", "", "", "", "", "", true),
            region("asia", "custom", "", "", "", "AS", "", false),
            region("cn-only", "custom", "CN", "", "", "", "CN", false),
        ];
        let picked = pick_region(&regions, "AS", "CN", "", "", "").unwrap();
        assert_eq!(picked.code, "cn-only");
    }

    #[test]
    fn standard_isp_still_beats_broader_standard_country() {
        let regions = vec![
            region("default", "default", "", "", "", "", "", true),
            region("cn", "country", "CN", "", "", "", "", false),
            region("cn-telecom", "isp", "CN", "", "电信", "", "", false),
        ];
        let picked = pick_region(&regions, "AS", "CN", "广东", "", "电信").unwrap();
        assert_eq!(picked.code, "cn-telecom");
    }

    /// IP-library country codes (MaxMind ISO) must select the DNS line that
    /// stores the same ISO in `countries` / `country` — the path used by
    /// `/dns/resolve-line` and schedule-zone A filtering.
    #[test]
    fn maxmind_style_us_and_tw_select_matching_lines() {
        let regions = vec![
            region("default", "default", "", "", "", "", "", true),
            region("area-us", "custom", "US", "", "", "NA", "US", false),
            region("area-tw", "custom", "TW", "", "", "AS", "TW", false),
        ];
        assert_eq!(
            pick_region(&regions, "NA", "US", "", "", "").unwrap().code,
            "area-us"
        );
        assert_eq!(
            pick_region(&regions, "AS", "TW", "", "", "").unwrap().code,
            "area-tw"
        );
        // Empty country (IP library miss) must not fake a line match.
        assert_eq!(
            pick_region(&regions, "", "", "", "", "").unwrap().code,
            "default"
        );
    }
}
