//! Remote edge-node management over SSH: credential storage, the SSH client
//! and the background operation runner.

pub mod crypto;
pub mod dns_ops;
pub mod ops;
pub mod ssh;

use crate::repository::SshGrantRow;

/// Decrypt a stored credential into SSH login data.
pub fn credentials_from_grant(
    jwt_secret: &str,
    grant: &SshGrantRow,
) -> Result<ssh::Credentials, String> {
    let secret = crypto::decrypt(jwt_secret, &grant.secret_enc)?;
    if secret.is_empty() {
        return Err(format!("SSH 认证「{}」没有保存密码或私钥", grant.name));
    }
    let secret = match grant.method.as_str() {
        "key" => ssh::Secret::Key {
            pem: secret,
            passphrase: crypto::decrypt(jwt_secret, &grant.passphrase_enc)?,
        },
        _ => ssh::Secret::Password(secret),
    };
    Ok(ssh::Credentials {
        username: grant.username.clone(),
        secret,
        use_sudo: grant.use_sudo,
    })
}
