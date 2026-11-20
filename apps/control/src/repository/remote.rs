//! Remote node management: SSH credentials, node SSH endpoints and the
//! remote-operation log.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SshGrantRow {
    pub id: Uuid,
    pub name: String,
    pub method: String,
    pub username: String,
    pub secret_enc: String,
    pub passphrase_enc: String,
    pub use_sudo: bool,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[sqlx(default)]
    pub node_count: i64,
}

const GRANT_COLUMNS: &str = "g.id, g.name, g.method, g.username, g.secret_enc, g.passphrase_enc, \
     g.use_sudo, g.description, g.created_at, g.updated_at, \
     ((SELECT COUNT(*) FROM node_ssh s WHERE s.grant_id = g.id)
      + (SELECT COUNT(*) FROM dns_node_ssh d WHERE d.grant_id = g.id)) AS node_count";

pub async fn list_ssh_grants(pool: &PgPool) -> sqlx::Result<Vec<SshGrantRow>> {
    sqlx::query_as::<_, SshGrantRow>(&format!(
        "SELECT {GRANT_COLUMNS} FROM ssh_grants g ORDER BY g.name"
    ))
    .fetch_all(pool)
    .await
}

pub async fn get_ssh_grant(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<SshGrantRow>> {
    sqlx::query_as::<_, SshGrantRow>(&format!(
        "SELECT {GRANT_COLUMNS} FROM ssh_grants g WHERE g.id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn create_ssh_grant(
    pool: &PgPool,
    name: &str,
    method: &str,
    username: &str,
    secret_enc: &str,
    passphrase_enc: &str,
    use_sudo: bool,
    description: &str,
) -> sqlx::Result<Uuid> {
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO ssh_grants (name, method, username, secret_enc, passphrase_enc, use_sudo, description)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(name)
    .bind(method)
    .bind(username)
    .bind(secret_enc)
    .bind(passphrase_enc)
    .bind(use_sudo)
    .bind(description)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

#[allow(clippy::too_many_arguments)]
pub async fn update_ssh_grant(
    pool: &PgPool,
    id: Uuid,
    name: &str,
    method: &str,
    username: &str,
    secret_enc: &str,
    passphrase_enc: &str,
    use_sudo: bool,
    description: &str,
) -> sqlx::Result<bool> {
    let r = sqlx::query(
        "UPDATE ssh_grants SET name = $2, method = $3, username = $4, secret_enc = $5,
             passphrase_enc = $6, use_sudo = $7, description = $8, updated_at = NOW()
         WHERE id = $1",
    )
    .bind(id)
    .bind(name)
    .bind(method)
    .bind(username)
    .bind(secret_enc)
    .bind(passphrase_enc)
    .bind(use_sudo)
    .bind(description)
    .execute(pool)
    .await?;
    Ok(r.rows_affected() > 0)
}

pub async fn delete_ssh_grant(pool: &PgPool, id: Uuid) -> sqlx::Result<bool> {
    let r = sqlx::query("DELETE FROM ssh_grants WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(r.rows_affected() > 0)
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct NodeSshRow {
    pub node_id: Uuid,
    pub host: String,
    pub port: i32,
    pub grant_id: Option<Uuid>,
    pub host_key: String,
}

pub async fn get_node_ssh(pool: &PgPool, node_id: Uuid) -> sqlx::Result<Option<NodeSshRow>> {
    sqlx::query_as::<_, NodeSshRow>(
        "SELECT node_id, host, port, grant_id, host_key FROM node_ssh WHERE node_id = $1",
    )
    .bind(node_id)
    .fetch_optional(pool)
    .await
}

pub async fn list_node_ssh(pool: &PgPool) -> sqlx::Result<Vec<NodeSshRow>> {
    sqlx::query_as::<_, NodeSshRow>("SELECT node_id, host, port, grant_id, host_key FROM node_ssh")
        .fetch_all(pool)
        .await
}

/// Insert or update a node's SSH endpoint. A changed host or port clears the
/// pinned host key: it belongs to the old address.
pub async fn upsert_node_ssh(
    pool: &PgPool,
    node_id: Uuid,
    host: &str,
    port: i32,
    grant_id: Option<Uuid>,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO node_ssh (node_id, host, port, grant_id)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (node_id) DO UPDATE SET
             host_key = CASE
                 WHEN node_ssh.host = EXCLUDED.host AND node_ssh.port = EXCLUDED.port
                 THEN node_ssh.host_key ELSE '' END,
             host = EXCLUDED.host,
             port = EXCLUDED.port,
             grant_id = EXCLUDED.grant_id,
             updated_at = NOW()",
    )
    .bind(node_id)
    .bind(host)
    .bind(port)
    .bind(grant_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_node_ssh(pool: &PgPool, node_id: Uuid) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM node_ssh WHERE node_id = $1")
        .bind(node_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_node_host_key(pool: &PgPool, node_id: Uuid, host_key: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE node_ssh SET host_key = $2, updated_at = NOW() WHERE node_id = $1")
        .bind(node_id)
        .bind(host_key)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct NodeOpRow {
    pub id: Uuid,
    pub node_id: Uuid,
    pub node_name: String,
    pub action: String,
    pub status: String,
    pub params: String,
    pub log: String,
    pub error: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

const OP_COLUMNS: &str = "o.id, o.node_id, COALESCE(n.name, '') AS node_name, o.action, o.status, \
     o.params, o.log, o.error, o.created_by, o.created_at, o.started_at, o.finished_at";

/// Queue an operation. Returns `None` when the node already has an active one
/// (the partial unique index rejects the insert).
pub async fn create_node_op(
    pool: &PgPool,
    node_id: Uuid,
    action: &str,
    params: &str,
    created_by: &str,
) -> sqlx::Result<Option<Uuid>> {
    let result = sqlx::query_as::<_, (Uuid,)>(
        "INSERT INTO node_ops (node_id, action, params, created_by)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(node_id)
    .bind(action)
    .bind(params)
    .bind(created_by)
    .fetch_one(pool)
    .await;
    match result {
        Ok((id,)) => Ok(Some(id)),
        Err(sqlx::Error::Database(db)) if db.constraint() == Some("node_ops_one_active_idx") => {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

pub async fn get_node_op(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<NodeOpRow>> {
    sqlx::query_as::<_, NodeOpRow>(&format!(
        "SELECT {OP_COLUMNS} FROM node_ops o LEFT JOIN edge_nodes n ON n.id = o.node_id WHERE o.id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Recent operations, newest first; optionally for one node.
pub async fn list_node_ops(
    pool: &PgPool,
    node_id: Option<Uuid>,
    limit: i64,
) -> sqlx::Result<Vec<NodeOpRow>> {
    sqlx::query_as::<_, NodeOpRow>(&format!(
        "SELECT {OP_COLUMNS} FROM node_ops o LEFT JOIN edge_nodes n ON n.id = o.node_id
         WHERE ($1::uuid IS NULL OR o.node_id = $1)
         ORDER BY o.created_at DESC LIMIT $2"
    ))
    .bind(node_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// The newest operation of every node (for the node list).
pub async fn latest_node_ops(pool: &PgPool) -> sqlx::Result<Vec<NodeOpRow>> {
    sqlx::query_as::<_, NodeOpRow>(&format!(
        "SELECT DISTINCT ON (o.node_id) {OP_COLUMNS}
         FROM node_ops o LEFT JOIN edge_nodes n ON n.id = o.node_id
         ORDER BY o.node_id, o.created_at DESC"
    ))
    .fetch_all(pool)
    .await
}

pub async fn mark_node_op_running(pool: &PgPool, id: Uuid) -> sqlx::Result<()> {
    sqlx::query("UPDATE node_ops SET status = 'running', started_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Append to the operation log, keeping only the last 64 KiB.
pub async fn append_node_op_log(pool: &PgPool, id: Uuid, chunk: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE node_ops SET log = RIGHT(log || $2, 65536) WHERE id = $1")
        .bind(id)
        .bind(chunk)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn finish_node_op(
    pool: &PgPool,
    id: Uuid,
    success: bool,
    error: &str,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE node_ops SET status = $2, error = $3, finished_at = NOW() WHERE id = $1")
        .bind(id)
        .bind(if success { "success" } else { "failed" })
        .bind(error)
        .execute(pool)
        .await?;
    Ok(())
}

/// Operations left queued/running by a previous control process can never
/// finish: fail them so the nodes accept new operations.
pub async fn fail_interrupted_node_ops(pool: &PgPool) -> sqlx::Result<u64> {
    let edge = sqlx::query(
        "UPDATE node_ops SET status = 'failed', error = '控制面重启，任务被中断', finished_at = NOW()
         WHERE status IN ('queued', 'running')",
    )
    .execute(pool)
    .await?
    .rows_affected();
    let dns = sqlx::query(
        "UPDATE dns_node_ops SET status = 'failed', error = '控制面重启，任务被中断', finished_at = NOW()
         WHERE status IN ('queued', 'running')",
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok(edge + dns)
}

// ---------------------------------------------------------------------------
// DNS node SSH / ops (same shape as edge; separate tables, shared ssh_grants)
// ---------------------------------------------------------------------------

pub async fn get_dns_node_ssh(pool: &PgPool, node_id: Uuid) -> sqlx::Result<Option<NodeSshRow>> {
    sqlx::query_as::<_, NodeSshRow>(
        "SELECT node_id, host, port, grant_id, host_key FROM dns_node_ssh WHERE node_id = $1",
    )
    .bind(node_id)
    .fetch_optional(pool)
    .await
}

pub async fn list_dns_node_ssh(pool: &PgPool) -> sqlx::Result<Vec<NodeSshRow>> {
    sqlx::query_as::<_, NodeSshRow>(
        "SELECT node_id, host, port, grant_id, host_key FROM dns_node_ssh",
    )
    .fetch_all(pool)
    .await
}

pub async fn upsert_dns_node_ssh(
    pool: &PgPool,
    node_id: Uuid,
    host: &str,
    port: i32,
    grant_id: Option<Uuid>,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO dns_node_ssh (node_id, host, port, grant_id)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (node_id) DO UPDATE SET
             host_key = CASE
                 WHEN dns_node_ssh.host = EXCLUDED.host AND dns_node_ssh.port = EXCLUDED.port
                 THEN dns_node_ssh.host_key ELSE '' END,
             host = EXCLUDED.host,
             port = EXCLUDED.port,
             grant_id = EXCLUDED.grant_id,
             updated_at = NOW()",
    )
    .bind(node_id)
    .bind(host)
    .bind(port)
    .bind(grant_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_dns_node_ssh(pool: &PgPool, node_id: Uuid) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM dns_node_ssh WHERE node_id = $1")
        .bind(node_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_dns_node_host_key(
    pool: &PgPool,
    node_id: Uuid,
    host_key: &str,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE dns_node_ssh SET host_key = $2, updated_at = NOW() WHERE node_id = $1")
        .bind(node_id)
        .bind(host_key)
        .execute(pool)
        .await?;
    Ok(())
}

const DNS_OP_COLUMNS: &str = "o.id, o.node_id, COALESCE(n.name, '') AS node_name, o.action, o.status, \
     o.params, o.log, o.error, o.created_by, o.created_at, o.started_at, o.finished_at";

pub async fn create_dns_node_op(
    pool: &PgPool,
    node_id: Uuid,
    action: &str,
    params: &str,
    created_by: &str,
) -> sqlx::Result<Option<Uuid>> {
    let result = sqlx::query_as::<_, (Uuid,)>(
        "INSERT INTO dns_node_ops (node_id, action, params, created_by)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(node_id)
    .bind(action)
    .bind(params)
    .bind(created_by)
    .fetch_one(pool)
    .await;
    match result {
        Ok((id,)) => Ok(Some(id)),
        Err(sqlx::Error::Database(db))
            if db.constraint() == Some("dns_node_ops_one_active_idx") =>
        {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

pub async fn get_dns_node_op(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<NodeOpRow>> {
    sqlx::query_as::<_, NodeOpRow>(&format!(
        "SELECT {DNS_OP_COLUMNS} FROM dns_node_ops o
         LEFT JOIN dns_nodes n ON n.id = o.node_id WHERE o.id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn list_dns_node_ops(
    pool: &PgPool,
    node_id: Option<Uuid>,
    limit: i64,
) -> sqlx::Result<Vec<NodeOpRow>> {
    sqlx::query_as::<_, NodeOpRow>(&format!(
        "SELECT {DNS_OP_COLUMNS} FROM dns_node_ops o
         LEFT JOIN dns_nodes n ON n.id = o.node_id
         WHERE ($1::uuid IS NULL OR o.node_id = $1)
         ORDER BY o.created_at DESC LIMIT $2"
    ))
    .bind(node_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub async fn latest_dns_node_ops(pool: &PgPool) -> sqlx::Result<Vec<NodeOpRow>> {
    sqlx::query_as::<_, NodeOpRow>(&format!(
        "SELECT DISTINCT ON (o.node_id) {DNS_OP_COLUMNS}
         FROM dns_node_ops o LEFT JOIN dns_nodes n ON n.id = o.node_id
         ORDER BY o.node_id, o.created_at DESC"
    ))
    .fetch_all(pool)
    .await
}

pub async fn mark_dns_node_op_running(pool: &PgPool, id: Uuid) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE dns_node_ops SET status = 'running', started_at = NOW() WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn append_dns_node_op_log(pool: &PgPool, id: Uuid, chunk: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE dns_node_ops SET log = RIGHT(log || $2, 65536) WHERE id = $1")
        .bind(id)
        .bind(chunk)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn finish_dns_node_op(
    pool: &PgPool,
    id: Uuid,
    success: bool,
    error: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE dns_node_ops SET status = $2, error = $3, finished_at = NOW() WHERE id = $1",
    )
    .bind(id)
    .bind(if success { "success" } else { "failed" })
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DnsNodeRemoteRow {
    pub id: Uuid,
    pub name: String,
    pub public_ip: String,
    pub listen: String,
    pub status: String,
    pub version: String,
    pub last_seen_at: Option<DateTime<Utc>>,
}

pub async fn list_dns_nodes_for_remote(pool: &PgPool) -> sqlx::Result<Vec<DnsNodeRemoteRow>> {
    sqlx::query_as::<_, DnsNodeRemoteRow>(
        "SELECT id, name, public_ip, listen, status, version, last_seen_at
         FROM dns_nodes ORDER BY name",
    )
    .fetch_all(pool)
    .await
}

pub async fn get_dns_node_for_remote(
    pool: &PgPool,
    id: Uuid,
) -> sqlx::Result<Option<DnsNodeRemoteRow>> {
    sqlx::query_as::<_, DnsNodeRemoteRow>(
        "SELECT id, name, public_ip, listen, status, version, last_seen_at
         FROM dns_nodes WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn set_node_agent_url(pool: &PgPool, node_id: Uuid, agent_url: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE edge_nodes SET agent_url = $2, updated_at = NOW() WHERE id = $1")
        .bind(node_id)
        .bind(agent_url)
        .execute(pool)
        .await?;
    Ok(())
}

/// DNS-node operations left queued/running by a previous process (see
/// `fail_interrupted_node_ops`): fail them so the nodes accept new operations.
pub async fn fail_interrupted_dns_node_ops(pool: &PgPool) -> sqlx::Result<u64> {
    let r = sqlx::query(
        "UPDATE dns_node_ops SET status = 'failed', error = '控制面重启，任务被中断', finished_at = NOW()
         WHERE status IN ('queued', 'running')",
    )
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}
