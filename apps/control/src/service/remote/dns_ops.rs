//! Remote operations on authoritative DNS nodes over SSH.
//!
//! Reuses global `ssh_grants`; install scripts target `lagran-dns` / dns.env.

use super::ops::{action_label, ACTIONS, OpParams};
use super::ssh;
use crate::repository;
use crate::state::AppState;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::{mpsc, Semaphore};
use uuid::Uuid;

const MAX_CONCURRENT: usize = 10;
const OP_TIMEOUT: Duration = Duration::from_secs(20 * 60);

fn semaphore() -> Arc<Semaphore> {
    static SEM: OnceLock<Arc<Semaphore>> = OnceLock::new();
    SEM.get_or_init(|| Arc::new(Semaphore::new(MAX_CONCURRENT)))
        .clone()
}

/// Fail operations a previous process left behind (called at startup).
pub async fn recover(pool: &sqlx::PgPool) {
    match repository::fail_interrupted_dns_node_ops(pool).await {
        Ok(0) => {}
        Ok(n) => tracing::warn!(count = n, "failed DNS remote operations interrupted by a restart"),
        Err(e) => tracing::warn!(error = %e, "could not clean up interrupted DNS remote operations"),
    }
}

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
            repository::finish_dns_node_op(&state.pool, op_id, outcome.is_ok(), &error).await
        {
            tracing::error!(error = %e, %op_id, "record dns remote operation result");
        }
    });
}

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
                    let _ = repository::append_dns_node_op_log(&pool, op_id, &buf).await;
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
    let op = repository::get_dns_node_op(&state.pool, op_id)
        .await
        .map_err(|e| format!("读取任务失败：{e}"))?
        .ok_or("任务不存在")?;
    repository::mark_dns_node_op_running(&state.pool, op_id)
        .await
        .map_err(|e| format!("更新任务状态失败：{e}"))?;
    let params: OpParams = serde_json::from_str(&op.params).unwrap_or_default();

    let node = repository::get_dns_node_for_remote(&state.pool, op.node_id)
        .await
        .map_err(|e| format!("读取 DNS 节点失败：{e}"))?
        .ok_or("DNS 节点不存在")?;
    let target = repository::get_dns_node_ssh(&state.pool, node.id)
        .await
        .map_err(|e| format!("读取 SSH 设置失败：{e}"))?
        .ok_or("节点没有配置 SSH 地址")?;
    let grant_id = target.grant_id.ok_or("节点没有选择 SSH 认证")?;
    let grant = repository::get_ssh_grant(&state.pool, grant_id)
        .await
        .map_err(|e| format!("读取 SSH 认证失败：{e}"))?
        .ok_or("SSH 认证已被删除")?;
    let credentials = super::credentials_from_grant(&state.config.jwt_secret, &grant)?;

    if let ssh::Secret::Password(p) = &credentials.secret {
        log.set_secrets(vec![p.clone(), state.config.agent_token.clone()]);
    } else {
        log.set_secrets(vec![state.config.agent_token.clone()]);
    }

    log.line(&format!(
        "==> {}DNS 节点 {}（{}:{}，认证「{}」）",
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
        let _ = repository::set_dns_node_host_key(&state.pool, node.id, &session.fingerprint).await;
        log.line("==> 首次连接，已记录主机指纹");
    }

    let script = match op.action.as_str() {
        "install" => install_script(state, &node, &params)?,
        "reinstall" => format!("{}\n{}", uninstall_script(), install_script(state, &node, &params)?),
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
  env LAGRAN_REPO="$LAGRAN_REPO" {token_env} bash /tmp/lagran-install.sh dns -y "$@"
}}
"#,
        repo_q = sq(repo)
    )
}

fn render_dns_env(
    state: &AppState,
    node: &repository::DnsNodeRemoteRow,
    params: &OpParams,
) -> Result<String, String> {
    if state.config.agent_token.trim().is_empty() {
        return Err("控制面未配置 AGENT_TOKEN，无法生成 dns.env".into());
    }
    let control_url = if params.control_url.trim().is_empty() {
        "http://127.0.0.1:8080".to_string()
    } else {
        params.control_url.trim().trim_end_matches('/').to_string()
    };
    let listen = if node.listen.trim().is_empty() {
        "0.0.0.0:53"
    } else {
        node.listen.trim()
    };
    // lagran-dns refuses plain HTTP to a non-loopback control plane unless it
    // is explicitly allowed (the agent token would cross the network in clear).
    let insecure = control_url.starts_with("http://") && !control_url_is_loopback(&control_url);
    let lines = [
        ("DNS_LISTEN", listen.to_string()),
        (
            "DNS_CONTROL_INSECURE",
            if insecure { "true" } else { "false" }.to_string(),
        ),
        ("CONTROL_URL", control_url),
        ("AGENT_TOKEN", state.config.agent_token.clone()),
        ("DNS_SYNC_SECS", "15".into()),
        ("DNS_PUBLIC_IP", node.public_ip.clone()),
        ("DNS_NODE_NAME", node.name.clone()),
    ];
    let mut env = String::from("# Generated by the Lagran control plane (DNS remote install)\n");
    for (key, value) in lines {
        if value.contains('\n') || value.contains('\r') {
            return Err(format!("{key} contains a line break"));
        }
        env.push_str(&format!("{key}={value}\n"));
    }
    Ok(env)
}

fn control_url_is_loopback(url: &str) -> bool {
    let rest = url
        .split_once("://")
        .map(|(_, r)| r)
        .unwrap_or(url);
    let authority = rest.split('/').next().unwrap_or("");
    let host = if let Some(v6) = authority.strip_prefix('[') {
        v6.split(']').next().unwrap_or("")
    } else {
        authority.rsplit_once(':').map(|(h, _)| h).unwrap_or(authority)
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false)
}

fn install_script(
    state: &AppState,
    node: &repository::DnsNodeRemoteRow,
    params: &OpParams,
) -> Result<String, String> {
    let env = render_dns_env(state, node, params)?;
    if env.contains("LAGRAN_DNS_ENV_EOF") {
        return Err("dns.env contains the heredoc marker".into());
    }
    let version = params
        .version
        .as_deref()
        .map(|v| format!("--version {}", sq(v)))
        .unwrap_or_default();
    let repo = require_update_repo(params)?;
    Ok(format!(
        r#"{prelude}
echo "==> 写入 /etc/lagran/dns.env"
install -d -m 0750 /etc/lagran
( umask 077; cat > /etc/lagran/dns.env <<'LAGRAN_DNS_ENV_EOF'
{env}LAGRAN_DNS_ENV_EOF
)
chmod 600 /etc/lagran/dns.env
fetch_installer
echo "==> 运行安装脚本"
run_installer {version}
systemctl restart lagran-dns
sleep 2
if systemctl is-active --quiet lagran-dns; then
  echo "==> lagran-dns 运行中"
else
  journalctl -u lagran-dns -n 30 --no-pager || true
  echo "lagran-dns 未能启动" >&2
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
if [ ! -f /etc/lagran/dns.env ]; then
  echo "本机没有安装权威 DNS（缺少 /etc/lagran/dns.env），请使用「安装」" >&2
  exit 1
fi
fetch_installer
echo "==> 升级权威 DNS {version}"
run_installer {version}
systemctl restart lagran-dns
sleep 2
systemctl is-active --quiet lagran-dns && echo "==> lagran-dns 运行中"
/opt/lagran/bin/lagran-dns --version 2>/dev/null || true
"#,
        prelude = installer_prelude(repo)
    ))
}

const START_SCRIPT: &str = r#"set -e
systemctl enable --now lagran-dns
sleep 2
if systemctl is-active --quiet lagran-dns; then
  echo "==> lagran-dns 已启动"
else
  journalctl -u lagran-dns -n 30 --no-pager || true
  echo "lagran-dns 未能启动" >&2
  exit 1
fi
"#;

const STOP_SCRIPT: &str = r#"set -e
systemctl stop lagran-dns
echo "==> lagran-dns 已停止（开机仍会自动启动，如需禁用请卸载）"
"#;

fn uninstall_script() -> String {
    r#"echo "==> 停止并移除 lagran-dns"
systemctl disable --now lagran-dns.service 2>/dev/null || true
rm -f /etc/systemd/system/lagran-dns.service
systemctl daemon-reload
rm -f /opt/lagran/bin/lagran-dns /etc/lagran/dns.env /etc/lagran/dns.env.example
echo "==> 已卸载（同机 edge/control 未改动）"
"#
    .to_string()
}

pub fn actions() -> &'static [&'static str] {
    ACTIONS
}
