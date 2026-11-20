//! Remote operations on edge nodes, executed over SSH in the background.
//!
//! Each operation is a row in `node_ops`; the runner streams the remote output
//! into its log so the console can show progress. At most `MAX_CONCURRENT`
//! operations run at once (batch upgrades of many nodes queue up), and the
//! database allows only one active operation per node.

use super::ssh;
use crate::repository;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::{mpsc, Semaphore};
use uuid::Uuid;

const MAX_CONCURRENT: usize = 10;
const OP_TIMEOUT: Duration = Duration::from_secs(20 * 60);

pub const ACTIONS: &[&str] = &[
    "install",
    "upgrade",
    "reinstall",
    "start",
    "stop",
    "uninstall",
];

pub fn action_label(action: &str) -> &'static str {
    match action {
        "install" => "安装",
        "upgrade" => "升级",
        "reinstall" => "重装",
        "start" => "启动",
        "stop" => "停止",
        "uninstall" => "卸载",
        _ => "操作",
    }
}

/// Inputs captured when the operation is created (the request context — public
/// URL, resolved release — is not available in the background task).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpParams {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub control_url: String,
    /// GitHub `owner/repo` captured when the op was queued (from console settings).
    #[serde(default)]
    pub update_repo: Option<String>,
}

fn semaphore() -> Arc<Semaphore> {
    static SEM: OnceLock<Arc<Semaphore>> = OnceLock::new();
    SEM.get_or_init(|| Arc::new(Semaphore::new(MAX_CONCURRENT)))
        .clone()
}

/// Start an already-queued operation in the background.
pub fn spawn(state: AppState, op_id: Uuid) {
    tokio::spawn(async move {
        let _permit = semaphore().acquire_owned().await;
        let log = OpLog::start(state.pool.clone(), op_id);
        let result = tokio::time::timeout(OP_TIMEOUT, run(&state, op_id, &log)).await;
        let outcome = match result {
            Ok(r) => r,
            Err(_) => Err(format!(
                "超过 {} 分钟未完成，已中止",
                OP_TIMEOUT.as_secs() / 60
            )),
        };
        match &outcome {
            Ok(()) => log.line("==> 完成"),
            Err(e) => log.line(&format!("==> 失败：{e}")),
        }
        log.finish().await;
        let error = outcome.as_ref().err().cloned().unwrap_or_default();
        if let Err(e) =
            repository::finish_node_op(&state.pool, op_id, outcome.is_ok(), &error).await
        {
            tracing::error!(error = %e, %op_id, "record remote operation result");
        }
    });
}

/// Fail operations a previous process left behind (called at startup).
pub async fn recover(pool: &sqlx::PgPool) {
    match repository::fail_interrupted_node_ops(pool).await {
        Ok(0) => {}
        Ok(n) => tracing::warn!(
            count = n,
            "failed remote operations interrupted by a restart"
        ),
        Err(e) => tracing::warn!(error = %e, "could not clean up interrupted remote operations"),
    }
}

/// Buffered log writer: output arrives in many small chunks, the database gets
/// at most one append per second.
struct OpLog {
    tx: mpsc::UnboundedSender<String>,
    writer: tokio::task::JoinHandle<()>,
    secrets: std::sync::Mutex<Vec<String>>,
}

impl OpLog {
    fn start(pool: sqlx::PgPool, op_id: Uuid) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let writer = tokio::spawn(async move {
            let mut buf = String::new();
            loop {
                let next = tokio::time::timeout(Duration::from_secs(1), rx.recv()).await;
                let closed = match next {
                    Ok(Some(chunk)) => {
                        buf.push_str(&chunk);
                        if buf.len() < 4096 {
                            continue;
                        }
                        false
                    }
                    Ok(None) => true,
                    Err(_) => false,
                };
                if !buf.is_empty() {
                    let _ = repository::append_node_op_log(&pool, op_id, &buf).await;
                    buf.clear();
                }
                if closed {
                    break;
                }
            }
        });
        Self {
            tx,
            writer,
            secrets: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Values that must never reach the stored log (passwords, tokens).
    fn set_secrets(&self, secrets: Vec<String>) {
        *self.secrets.lock().unwrap() = secrets;
    }

    fn mask(&self, text: &str) -> String {
        let mut out = text.to_string();
        for secret in self.secrets.lock().unwrap().iter() {
            if secret.len() >= 6 {
                out = out.replace(secret.as_str(), "******");
            }
        }
        out
    }

    fn raw(&self, text: &str) {
        let _ = self.tx.send(self.mask(text));
    }

    fn line(&self, text: &str) {
        self.raw(&format!("{text}\n"));
    }

    async fn finish(self) {
        drop(self.tx);
        let _ = self.writer.await;
    }
}

async fn run(state: &AppState, op_id: Uuid, log: &OpLog) -> Result<(), String> {
    let op = repository::get_node_op(&state.pool, op_id)
        .await
        .map_err(|e| format!("读取任务失败：{e}"))?
        .ok_or("任务不存在")?;
    repository::mark_node_op_running(&state.pool, op_id)
        .await
        .map_err(|e| format!("更新任务状态失败：{e}"))?;
    let params: OpParams = serde_json::from_str(&op.params).unwrap_or_default();

    let node = repository::get_node(&state.pool, op.node_id)
        .await
        .map_err(|e| format!("读取节点失败：{e}"))?
        .ok_or("节点不存在")?;
    let target = repository::get_node_ssh(&state.pool, node.id)
        .await
        .map_err(|e| format!("读取 SSH 设置失败：{e}"))?
        .ok_or("节点没有配置 SSH 地址")?;
    let grant_id = target.grant_id.ok_or("节点没有选择 SSH 认证")?;
    let grant = repository::get_ssh_grant(&state.pool, grant_id)
        .await
        .map_err(|e| format!("读取 SSH 认证失败：{e}"))?
        .ok_or("SSH 认证已被删除")?;
    let credentials = super::credentials_from_grant(&state.config.jwt_secret, &grant)?;

    let mut log_secrets = vec![node.auth_token.clone()];
    if let ssh::Secret::Password(p) = &credentials.secret {
        log_secrets.push(p.clone());
    }
    if let Some(token) = crate::api::system::github_token() {
        log_secrets.push(token);
    }
    log.set_secrets(log_secrets);

    log.line(&format!(
        "==> {}节点 {}（{}:{}，认证「{}」）",
        action_label(&op.action),
        node.name,
        target.host,
        target.port,
        grant.name
    ));
    let pinned = (!target.host_key.is_empty()).then_some(target.host_key.as_str());
    let session = ssh::connect(&target.host, target.port as u16, &credentials, pinned).await?;
    log.line(&format!(
        "==> 已登录，主机指纹 {}，权限 {}",
        session.fingerprint,
        session.root_mode_label()
    ));
    if target.host_key.is_empty() && !session.fingerprint.is_empty() {
        let _ = repository::set_node_host_key(&state.pool, node.id, &session.fingerprint).await;
        log.line("==> 首次连接，已记录主机指纹");
    }

    let script = match op.action.as_str() {
        "install" => install_script(state, &node, &target, &params).await?,
        "reinstall" => format!(
            "{}\n{}",
            uninstall_script(),
            install_script(state, &node, &target, &params).await?
        ),
        "upgrade" => upgrade_script(&params)?,
        "start" => START_SCRIPT.to_string(),
        "stop" => STOP_SCRIPT.to_string(),
        "uninstall" => uninstall_script(),
        other => return Err(format!("未知操作 {other}")),
    };

    let mut sink = |text: &str| log.raw(text);
    let status = session.run_root_script(&script, &mut sink).await;
    session.close().await;
    match status? {
        0 => Ok(()),
        code => Err(format!("远程脚本退出码 {code}，详见日志")),
    }
}

/// Shell single-quoting for values interpolated into the scripts.
fn sq(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn require_update_repo(params: &OpParams) -> Result<&str, String> {
    params
        .update_repo
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .ok_or_else(|| "未配置升级仓库：请在系统设置中填写 owner/repo".into())
}

fn installer_prelude(repo: &str) -> String {
    let auth = match crate::api::system::github_token() {
        Some(token) => format!("-H {}", sq(&format!("Authorization: Bearer {token}"))),
        None => String::new(),
    };
    let token_env = match crate::api::system::github_token() {
        Some(token) => format!("LAGRAN_GITHUB_TOKEN={}", sq(&token)),
        None => String::new(),
    };
    format!(
        r#"set -e
export LC_ALL=C
export DEBIAN_FRONTEND=noninteractive
LAGRAN_REPO={repo_q}
if ! command -v curl >/dev/null 2>&1; then
  echo "==> 安装 curl"
  if command -v apt-get >/dev/null 2>&1; then apt-get update -qq && apt-get install -y -qq curl ca-certificates tar
  elif command -v dnf >/dev/null 2>&1; then dnf install -y -q curl tar
  elif command -v yum >/dev/null 2>&1; then yum install -y -q curl tar
  else echo "缺少 curl，且无法自动安装" >&2; exit 1; fi
fi
fetch_installer() {{
  echo "==> 下载安装脚本 ($LAGRAN_REPO)"
  curl -fsSL {auth} "https://raw.githubusercontent.com/$LAGRAN_REPO/main/install.sh" -o /tmp/lagran-install.sh
}}
run_installer() {{
  env LAGRAN_REPO="$LAGRAN_REPO" {token_env} bash /tmp/lagran-install.sh edge -y "$@"
}}
"#,
        repo_q = sq(repo)
    )
}

/// The edge.env for this node: the console wizard's rendering when the node
/// was created there, otherwise one built from the control plane's settings.
async fn render_edge_env(
    state: &AppState,
    node: &repository::NodeRow,
    target: &repository::NodeSshRow,
    params: &OpParams,
) -> Result<String, String> {
    if let Some(env) = crate::api::node::edge_env_from_profile(&node.edge_profile) {
        return Ok(env);
    }
    let loopback = is_loopback(&target.host);
    let mut agent_url = node.agent_url.trim().to_string();
    if agent_url.is_empty() {
        agent_url = if loopback {
            "http://127.0.0.1:8082".to_string()
        } else {
            let host = if node.public_ip.trim().is_empty() {
                target.host.as_str()
            } else {
                node.public_ip.trim()
            };
            format!("http://{}:8081", host_for_url(host))
        };
        repository::set_node_agent_url(&state.pool, node.id, &agent_url)
            .await
            .map_err(|e| format!("保存 AGENT_URL 失败：{e}"))?;
    }
    let admin_port = agent_url
        .rsplit_once(':')
        .and_then(|(_, p)| p.trim_end_matches('/').parse::<u16>().ok())
        .unwrap_or(8081);
    let agent_host_loopback = agent_url.contains("://127.") || agent_url.contains("://localhost");
    let admin_listen = if agent_host_loopback {
        format!("127.0.0.1:{admin_port}")
    } else {
        // The agent URL may carry a NAT'd public IP that is not on any local
        // interface, so listen on all addresses (restrict with a firewall).
        format!("0.0.0.0:{admin_port}")
    };
    let control_url = if loopback {
        let bind = std::env::var("CONTROL_BIND").unwrap_or_default();
        let port = bind.rsplit_once(':').map(|(_, p)| p).unwrap_or("8080");
        format!("http://127.0.0.1:{port}")
    } else {
        params.control_url.clone()
    };
    let public_ip =
        if node.public_ip.trim().is_empty() && target.host.parse::<std::net::IpAddr>().is_ok() {
            target.host.clone()
        } else {
            node.public_ip.trim().to_string()
        };
    let lines = [
        ("EDGE_LISTEN", "0.0.0.0:80".to_string()),
        ("EDGE_HTTPS_LISTEN", "0.0.0.0:443".to_string()),
        ("EDGE_ADMIN_LISTEN", admin_listen),
        ("NODE_ID", node.name.clone()),
        (
            "NODE_REGION",
            if node.region.is_empty() {
                "default".into()
            } else {
                node.region.clone()
            },
        ),
        ("PUBLIC_IP", public_ip),
        ("CONTROL_URL", control_url),
        ("AGENT_URL", agent_url),
        ("AGENT_TOKEN", node.auth_token.clone()),
        (
            "WAF_RULEPACK_SECRET",
            state.config.waf_rulepack_secret.clone(),
        ),
        (
            "CHALLENGE_SECRET",
            super::crypto::edge_challenge_secret(&state.config.jwt_secret),
        ),
        ("CACHE_BACKEND", "memory".into()),
        (
            "EDGE_UPDATE_HELPER",
            "/opt/lagran/bin/lagran-edge-update".into(),
        ),
        (
            "EDGE_UPDATE_REQUEST_PATH",
            "/var/lib/lagran/edge-update.request".into(),
        ),
    ];
    let mut env = String::from("# Generated by the Lagran control plane (remote install)\n");
    for (key, value) in lines {
        if value.contains('\n') || value.contains('\r') {
            return Err(format!("{key} contains a line break"));
        }
        env.push_str(&format!("{key}={value}\n"));
    }
    Ok(env)
}

async fn install_script(
    state: &AppState,
    node: &repository::NodeRow,
    target: &repository::NodeSshRow,
    params: &OpParams,
) -> Result<String, String> {
    let env = render_edge_env(state, node, target, params).await?;
    if env.contains("LAGRAN_EDGE_ENV_EOF") {
        return Err("edge.env contains the heredoc marker".into());
    }
    let version = params
        .version
        .as_deref()
        .map(|v| format!("--version {}", sq(v)))
        .unwrap_or_default();
    let repo = require_update_repo(params)?;
    Ok(format!(
        r#"{prelude}
echo "==> 写入 /etc/lagran/edge.env"
install -d -m 0750 /etc/lagran
( umask 077; cat > /etc/lagran/edge.env <<'LAGRAN_EDGE_ENV_EOF'
{env}LAGRAN_EDGE_ENV_EOF
)
chmod 600 /etc/lagran/edge.env
fetch_installer
echo "==> 运行安装脚本"
run_installer {version}
systemctl restart lagran-edge
sleep 2
if systemctl is-active --quiet lagran-edge; then
  echo "==> lagran-edge 运行中"
else
  journalctl -u lagran-edge -n 30 --no-pager || true
  echo "lagran-edge 未能启动" >&2
  exit 1
fi
"#,
        prelude = installer_prelude(repo)
    ))
}

fn upgrade_script(params: &OpParams) -> Result<String, String> {
    let version = params
        .version
        .as_deref()
        .map(|v| format!("--version {}", sq(v)))
        .unwrap_or_default();
    let repo = require_update_repo(params)?;
    Ok(format!(
        r#"{prelude}
if [ ! -f /etc/lagran/edge.env ]; then
  echo "本机没有安装边缘节点（缺少 /etc/lagran/edge.env），请使用「安装」" >&2
  exit 1
fi
fetch_installer
echo "==> 升级边缘节点 {version}"
run_installer {version}
systemctl restart lagran-edge
sleep 2
systemctl is-active --quiet lagran-edge && echo "==> lagran-edge 运行中"
/opt/lagran/bin/lagran-edge --version 2>/dev/null || true
"#,
        prelude = installer_prelude(repo)
    ))
}

const START_SCRIPT: &str = r#"set -e
systemctl enable --now lagran-edge
sleep 2
if systemctl is-active --quiet lagran-edge; then
  echo "==> lagran-edge 已启动"
else
  journalctl -u lagran-edge -n 30 --no-pager || true
  echo "lagran-edge 未能启动" >&2
  exit 1
fi
"#;

const STOP_SCRIPT: &str = r#"set -e
systemctl stop lagran-edge
echo "==> lagran-edge 已停止（开机仍会自动启动，如需禁用请卸载）"
"#;

/// Removes the edge only. Control plane / DNS files on the same host are left
/// alone, as is /var/lib/lagran itself (shared by every component).
fn uninstall_script() -> String {
    r#"echo "==> 停止并移除 lagran-edge"
systemctl disable --now lagran-edge.service 2>/dev/null || true
systemctl disable --now lagran-edge-update.path 2>/dev/null || true
systemctl stop lagran-edge-update.service 2>/dev/null || true
rm -f /etc/systemd/system/lagran-edge.service \
      /etc/systemd/system/lagran-edge-update.service \
      /etc/systemd/system/lagran-edge-update.path
systemctl daemon-reload
rm -f /opt/lagran/bin/lagran-edge /opt/lagran/bin/lagran-edge-update \
      /etc/lagran/edge.env /etc/lagran/edge-update.env /etc/lagran/edge.env.example \
      /var/lib/lagran/edge-update.request /var/lib/lagran/edge-update.status
rm -rf /var/lib/lagran/cache /var/lib/lagran/large-cache
echo "==> 已卸载"
"#
    .to_string()
}

fn is_loopback(host: &str) -> bool {
    host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false)
}

fn host_for_url(host: &str) -> String {
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V6(_)) => format!("[{host}]"),
        _ => host.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_quote_escaping() {
        assert_eq!(sq("a'b"), "'a'\\''b'");
    }

    #[test]
    fn loopback_detection() {
        assert!(is_loopback("127.0.0.1"));
        assert!(is_loopback("::1"));
        assert!(is_loopback("localhost"));
        assert!(!is_loopback("10.0.0.1"));
    }

    #[test]
    fn uninstall_never_touches_shared_files() {
        let script = uninstall_script();
        assert!(!script.contains("control.env"));
        assert!(!script.contains("dns.env"));
        assert!(!script.contains("rm -rf /var/lib/lagran\n"));
    }
}
