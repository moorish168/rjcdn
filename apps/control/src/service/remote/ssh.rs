//! Minimal SSH client used to install and manage edge nodes.
//!
//! Unlike tools that accept any host key, the server key fingerprint is pinned:
//! the first successful connection records it, and later connections refuse a
//! different key (a re-imaged server needs "重置指纹" in the console).

use russh::client;
use russh::keys::{HashAlg, PrivateKeyWithHashAlg};
use russh::ChannelMsg;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub enum Secret {
    Password(String),
    Key { pem: String, passphrase: String },
}

#[derive(Clone)]
pub struct Credentials {
    pub username: String,
    pub secret: Secret,
    pub use_sudo: bool,
}

struct Client {
    pinned: Option<String>,
    seen: Arc<Mutex<Option<String>>>,
}

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let fingerprint = server_public_key
            .public_key()
            .fingerprint(HashAlg::Sha256)
            .to_string();
        let accepted = match self.pinned.as_deref() {
            Some(pinned) if !pinned.is_empty() => pinned == fingerprint,
            _ => true,
        };
        *self.seen.lock().unwrap() = Some(fingerprint);
        Ok(accepted)
    }
}

/// How commands get root: already root, passwordless sudo, or `sudo -S`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RootMode {
    Root,
    SudoNoPassword,
    SudoPassword,
    None,
}

pub struct Session {
    handle: client::Handle<Client>,
    credentials: Credentials,
    root: RootMode,
    /// SHA256 fingerprint of the server host key.
    pub fingerprint: String,
}

/// Connect and authenticate. `pinned` is the recorded fingerprint, if any.
pub async fn connect(
    host: &str,
    port: u16,
    credentials: &Credentials,
    pinned: Option<&str>,
) -> Result<Session, String> {
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(900)),
        keepalive_interval: Some(Duration::from_secs(30)),
        ..Default::default()
    });
    let seen = Arc::new(Mutex::new(None));
    let handler = Client {
        pinned: pinned.map(str::to_string),
        seen: seen.clone(),
    };
    let connected = tokio::time::timeout(
        CONNECT_TIMEOUT,
        client::connect(config, (host, port), handler),
    )
    .await;
    let mut handle = match connected {
        Err(_) => return Err(format!("连接 {host}:{port} 超时")),
        Ok(Err(e)) => {
            let fingerprint = seen.lock().unwrap().clone().unwrap_or_default();
            if let Some(pinned) = pinned.filter(|p| !p.is_empty()) {
                if !fingerprint.is_empty() && fingerprint != pinned {
                    return Err(format!(
                        "服务器 SSH 指纹与记录不一致（记录 {pinned}，当前 {fingerprint}）。\
                         如果服务器重装过，请在控制台重置指纹后重试"
                    ));
                }
            }
            return Err(format!("连接 {host}:{port} 失败：{e}"));
        }
        Ok(Ok(handle)) => handle,
    };
    let fingerprint = seen.lock().unwrap().clone().unwrap_or_default();

    let authenticated = match &credentials.secret {
        Secret::Password(password) => handle
            .authenticate_password(credentials.username.clone(), password.clone())
            .await
            .map_err(|e| format!("SSH 认证出错：{e}"))?
            .success(),
        Secret::Key { pem, passphrase } => {
            let key = russh::keys::decode_secret_key(
                pem,
                (!passphrase.is_empty()).then_some(passphrase.as_str()),
            )
            .map_err(|e| format!("私钥无法解析：{e}"))?;
            let hash = handle
                .best_supported_rsa_hash()
                .await
                .map_err(|e| format!("SSH 协商失败：{e}"))?
                .flatten();
            handle
                .authenticate_publickey(
                    credentials.username.clone(),
                    PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                )
                .await
                .map_err(|e| format!("SSH 认证出错：{e}"))?
                .success()
        }
    };
    if !authenticated {
        return Err(format!(
            "SSH 认证失败：用户 {} 的密码或私钥不正确",
            credentials.username
        ));
    }

    let mut session = Session {
        handle,
        credentials: credentials.clone(),
        root: RootMode::None,
        fingerprint,
    };
    session.root = session.detect_root().await;
    Ok(session)
}

impl Session {
    async fn detect_root(&self) -> RootMode {
        if self.credentials.username == "root" {
            return RootMode::Root;
        }
        if let Ok((0, out)) = self.exec_raw("id -u", None, &mut |_| {}).await {
            if out.trim() == "0" {
                return RootMode::Root;
            }
        }
        if !self.credentials.use_sudo {
            return RootMode::None;
        }
        if let Ok((0, _)) = self.exec_raw("sudo -n true", None, &mut |_| {}).await {
            return RootMode::SudoNoPassword;
        }
        match self.credentials.secret {
            Secret::Password(_) => RootMode::SudoPassword,
            Secret::Key { .. } => RootMode::None,
        }
    }

    /// Whether root commands can run, with a reason when they cannot.
    pub fn root_problem(&self) -> Option<String> {
        match self.root {
            RootMode::None if !self.credentials.use_sudo => Some(format!(
                "用户 {} 不是 root，且认证未勾选「使用 sudo」",
                self.credentials.username
            )),
            RootMode::None => Some(format!(
                "用户 {} 的 sudo 需要密码，但认证方式是私钥；请配置免密 sudo 或改用 root",
                self.credentials.username
            )),
            _ => None,
        }
    }

    pub fn root_mode_label(&self) -> &'static str {
        match self.root {
            RootMode::Root => "root",
            RootMode::SudoNoPassword => "sudo（免密）",
            RootMode::SudoPassword => "sudo（密码）",
            RootMode::None => "普通用户",
        }
    }

    /// Run `command` and stream its output to `on_output`. Returns the exit
    /// status and the full output.
    async fn exec_raw(
        &self,
        command: &str,
        stdin: Option<&[u8]>,
        on_output: &mut (dyn FnMut(&str) + Send),
    ) -> Result<(u32, String), String> {
        let mut channel = self
            .handle
            .channel_open_session()
            .await
            .map_err(|e| format!("打开 SSH 会话失败：{e}"))?;
        channel
            .exec(true, command)
            .await
            .map_err(|e| format!("执行命令失败：{e}"))?;
        if let Some(data) = stdin {
            channel
                .data(data)
                .await
                .map_err(|e| format!("发送数据失败：{e}"))?;
        }
        channel.eof().await.ok();
        let mut output = String::new();
        let mut status = None;
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { ref data } | ChannelMsg::ExtendedData { ref data, .. } => {
                    let text = String::from_utf8_lossy(data);
                    on_output(&text);
                    if output.len() < 256 * 1024 {
                        output.push_str(&text);
                    }
                }
                ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        Ok((status.unwrap_or(255), output))
    }

    /// Run a bash script as root (directly, or through sudo). The script is
    /// sent on stdin so secrets in it never appear in the remote process list.
    pub async fn run_root_script(
        &self,
        script: &str,
        on_output: &mut (dyn FnMut(&str) + Send),
    ) -> Result<u32, String> {
        if let Some(problem) = self.root_problem() {
            return Err(problem);
        }
        let (command, input) = match (self.root, &self.credentials.secret) {
            (RootMode::Root, _) => ("bash -s".to_string(), script.to_string()),
            (RootMode::SudoNoPassword, _) => ("sudo -n bash -s".to_string(), script.to_string()),
            (RootMode::SudoPassword, Secret::Password(password)) => (
                // sudo reads the password line from stdin; bash reads the rest.
                "sudo -S -p '' bash -s".to_string(),
                format!("{password}\n{script}"),
            ),
            _ => return Err("无法获得 root 权限".into()),
        };
        let (status, _) = self
            .exec_raw(&command, Some(input.as_bytes()), on_output)
            .await?;
        Ok(status)
    }

    /// Run a harmless command as the login user (connection test).
    pub async fn run_user(&self, command: &str) -> Result<(u32, String), String> {
        self.exec_raw(command, None, &mut |_| {}).await
    }

    pub async fn close(self) {
        let _ = self
            .handle
            .disconnect(russh::Disconnect::ByApplication, "", "en")
            .await;
    }
}

/// Live tests against a disposable sshd, run with:
/// `SSH_TEST_PORT=2222 cargo test -p control --lib remote::ssh -- --ignored`
/// (users: root/Rootpass123, ops/Opspass123 with password sudo, nosudo/Nosudo123).
#[cfg(test)]
mod live_tests {
    use super::*;

    fn port() -> Option<u16> {
        std::env::var("SSH_TEST_PORT").ok()?.parse().ok()
    }

    fn creds(user: &str, password: &str, use_sudo: bool) -> Credentials {
        Credentials {
            username: user.into(),
            secret: Secret::Password(password.into()),
            use_sudo,
        }
    }

    async fn run(session: &Session, script: &str) -> (u32, String) {
        let mut out = String::new();
        let mut sink = |t: &str| out.push_str(t);
        let code = session.run_root_script(script, &mut sink).await.unwrap();
        (code, out)
    }

    #[tokio::test]
    #[ignore]
    async fn root_password_login_and_script() {
        let Some(port) = port() else { return };
        let s = connect(
            "127.0.0.1",
            port,
            &creds("root", "Rootpass123", false),
            None,
        )
        .await
        .unwrap();
        assert!(s.fingerprint.starts_with("SHA256:"));
        assert_eq!(s.root_mode_label(), "root");
        let (code, out) = run(&s, "echo hello; id -u; exit 3").await;
        assert_eq!(code, 3);
        assert!(out.contains("hello") && out.contains('0'), "{out}");
        s.close().await;
    }

    #[tokio::test]
    #[ignore]
    async fn sudo_with_password_reads_script_from_stdin() {
        let Some(port) = port() else { return };
        let s = connect("127.0.0.1", port, &creds("ops", "Opspass123", true), None)
            .await
            .unwrap();
        assert_eq!(s.root_mode_label(), "sudo（密码）");
        let (code, out) = run(&s, "id -u\nwhoami\n").await;
        assert_eq!(code, 0, "{out}");
        assert!(out.contains("root"), "{out}");
        assert!(!out.contains("Opspass123"), "password leaked: {out}");
        s.close().await;
    }

    #[tokio::test]
    #[ignore]
    async fn non_root_without_sudo_is_refused() {
        let Some(port) = port() else { return };
        let s = connect(
            "127.0.0.1",
            port,
            &creds("nosudo", "Nosudo123", false),
            None,
        )
        .await
        .unwrap();
        assert!(s.root_problem().is_some());
        s.close().await;
    }

    #[tokio::test]
    #[ignore]
    async fn wrong_password_and_pinned_key_mismatch() {
        let Some(port) = port() else { return };
        let bad = connect("127.0.0.1", port, &creds("root", "wrong", false), None).await;
        assert!(bad.err().unwrap().contains("认证失败"));
        let pinned = connect(
            "127.0.0.1",
            port,
            &creds("root", "Rootpass123", false),
            Some("SHA256:not-the-real-key"),
        )
        .await;
        let e = pinned.err().unwrap();
        assert!(e.contains("指纹"), "{e}");
    }
}

#[cfg(test)]
mod live_key_tests {
    use super::*;

    /// `SSH_TEST_KEY_DIR` holds id_test (ed25519, no passphrase) and
    /// id_rsa_test (RSA, passphrase "keypass1"), both authorized for root.
    async fn login(file: &str, passphrase: &str) -> Result<Session, String> {
        let port: u16 = std::env::var("SSH_TEST_PORT").unwrap().parse().unwrap();
        let dir = std::env::var("SSH_TEST_KEY_DIR").unwrap();
        let pem = std::fs::read_to_string(format!("{dir}/{file}")).unwrap();
        let creds = Credentials {
            username: "root".into(),
            secret: Secret::Key {
                pem,
                passphrase: passphrase.into(),
            },
            use_sudo: false,
        };
        connect("127.0.0.1", port, &creds, None).await
    }

    #[tokio::test]
    #[ignore]
    async fn ed25519_and_rsa_keys() {
        if std::env::var("SSH_TEST_KEY_DIR").is_err() {
            return;
        }
        let s = login("id_test", "").await.unwrap();
        assert_eq!(s.root_mode_label(), "root");
        s.close().await;
        let s = login("id_rsa_test", "keypass1").await.unwrap();
        s.close().await;
        assert!(login("id_rsa_test", "wrong").await.is_err());
    }
}
