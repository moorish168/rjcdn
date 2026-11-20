//! DNS provider accounts used for CNAME-mode ACME DNS-01.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DnsProviderRow {
    pub id: Uuid,
    pub name: String,
    pub provider_type: String,
    pub credentials_enc: String,
    pub note: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn list_dns_providers(pool: &PgPool) -> sqlx::Result<Vec<DnsProviderRow>> {
    sqlx::query_as::<_, DnsProviderRow>(
        r#"
        SELECT id, name, provider_type, credentials_enc, note, enabled, created_at, updated_at
        FROM dns_providers
        ORDER BY name
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn get_dns_provider(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<DnsProviderRow>> {
    sqlx::query_as::<_, DnsProviderRow>(
        r#"
        SELECT id, name, provider_type, credentials_enc, note, enabled, created_at, updated_at
        FROM dns_providers WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn create_dns_provider(
    pool: &PgPool,
    name: &str,
    provider_type: &str,
    credentials_enc: &str,
    note: &str,
) -> sqlx::Result<Uuid> {
    let (id,): (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO dns_providers (name, provider_type, credentials_enc, note)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
    )
    .bind(name)
    .bind(provider_type)
    .bind(credentials_enc)
    .bind(note)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub async fn update_dns_provider(
    pool: &PgPool,
    id: Uuid,
    name: &str,
    credentials_enc: Option<&str>,
    note: &str,
    enabled: bool,
) -> sqlx::Result<u64> {
    let r = if let Some(creds) = credentials_enc {
        sqlx::query(
            r#"
            UPDATE dns_providers
            SET name = $2, credentials_enc = $3, note = $4, enabled = $5, updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(creds)
        .bind(note)
        .bind(enabled)
        .execute(pool)
        .await?
    } else {
        sqlx::query(
            r#"
            UPDATE dns_providers
            SET name = $2, note = $3, enabled = $4, updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(note)
        .bind(enabled)
        .execute(pool)
        .await?
    };
    Ok(r.rows_affected())
}

pub async fn delete_dns_provider(pool: &PgPool, id: Uuid) -> sqlx::Result<u64> {
    let r = sqlx::query("DELETE FROM dns_providers WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(r.rows_affected())
}

pub async fn get_domain_dns_provider_id(
    pool: &PgPool,
    domain_id: Uuid,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar::<_, Option<Uuid>>("SELECT dns_provider_id FROM domains WHERE id = $1")
        .bind(domain_id)
        .fetch_optional(pool)
        .await
        .map(|r| r.flatten())
}

pub async fn set_domain_dns_provider(
    pool: &PgPool,
    domain_id: Uuid,
    provider_id: Option<Uuid>,
) -> sqlx::Result<u64> {
    let r = sqlx::query(
        "UPDATE domains SET dns_provider_id = $2, updated_at = NOW() WHERE id = $1",
    )
    .bind(domain_id)
    .bind(provider_id)
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}
