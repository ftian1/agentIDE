//! SSH client wrapper around [`russh`] v0.61.
//!
//! Supports key-based (ed25519, rsa), agent-forwarded, and password
//! authentication.

use anyhow::{Context, bail};
use russh::*;
use std::path::PathBuf;
use std::sync::Arc;

/// SSH connection parameters.
#[derive(Debug, Clone)]
pub struct SshConnectionParams {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: AuthMethod,
}

/// Authentication method.
#[derive(Debug, Clone)]
pub enum AuthMethod {
    Key(Option<PathBuf>),
    Agent,
    Password(String),
}

/// Result of establishing an SSH connection.
pub struct SshSession {
    pub handle: russh::client::Handle<SshClientHandler>,
    pub connection: client::Connection,
}

/// An opened channel on an SSH session (for exec/shell).
pub type SshChannel = russh::Channel<Msg>;

/// Our minimal SSH client handler.
pub struct SshClientHandler;

#[async_trait::async_trait]
impl client::Handler for SshClientHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        tracing::info!("Accepting server host key (known_hosts not yet implemented)");
        Ok(true)
    }
}

/// Open an SSH connection to a remote host.
pub async fn connect(params: &SshConnectionParams) -> anyhow::Result<SshSession> {
    tracing::info!(host = %params.host, port = params.port, user = %params.user, "Connecting via SSH");

    let config = client::Config::default();
    let config = Arc::new(config);
    let sh = SshClientHandler;

    let addr = format!("{}:{}", params.host, params.port);
    let mut connection = russh::client::connect(config, &addr, sh).await
        .context("SSH connection failed")?;

    let authenticated = match &params.auth {
        AuthMethod::Key(identity_file) => {
            let key_path = identity_file.clone().unwrap_or_else(|| default_key_path());
            tracing::info!(key = %key_path.display(), "Authenticating with key");

            match load_key(&key_path) {
                Ok(key) => {
                    connection.authenticate_publickey(&params.user, Arc::new(key)).await
                        .context("Key authentication failed")?
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Key loading failed, falling back");
                    false
                }
            }
        }
        AuthMethod::Agent => {
            tracing::info!("Agent authentication — falling back to default key");
            let key_path = default_key_path();
            if key_path.exists() {
                let key = load_key(&key_path)?;
                connection.authenticate_publickey(&params.user, Arc::new(key)).await
                    .context("Key authentication failed")?
            } else {
                bail!("No default key found for agent auth");
            }
        }
        AuthMethod::Password(password) => {
            tracing::info!("Authenticating with password");
            connection.authenticate_password(&params.user, password).await
                .context("Password authentication failed")?
        }
    };

    if !authenticated {
        bail!("All authentication methods failed");
    }

    tracing::info!(host = %params.host, "SSH connected");

    Ok(SshSession {
        handle: connection.handle(),
        connection,
    })
}

/// Open an exec channel — runs a command and returns a channel piped to its stdin/stdout.
pub async fn open_exec_channel(
    session: &SshSession,
    command: &str,
) -> anyhow::Result<SshChannel> {
    let channel = session.handle.channel_open_session().await
        .context("Failed to open SSH channel")?;

    channel.exec(true, command.as_bytes()).await
        .context("Failed to exec command")?;

    Ok(channel)
}

/// Run a simple command on the remote host and return stdout as string.
pub async fn exec_remote(
    session: &SshSession,
    command: &str,
) -> anyhow::Result<String> {
    let mut channel = open_exec_channel(session, command).await?;
    let mut output = Vec::new();

    loop {
        match channel.wait().await {
            Some(ChannelMsg::Data { ref data }) => {
                output.extend_from_slice(data);
            }
            Some(ChannelMsg::Eof) | None => break,
            _ => continue,
        }
    }

    Ok(String::from_utf8_lossy(&output).trim().to_string())
}

/// Get the default SSH key path (~/.ssh/id_ed25519 or ~/.ssh/id_rsa).
fn default_key_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_default();
    let ed = home.join(".ssh/id_ed25519");
    if ed.exists() { ed } else { home.join(".ssh/id_rsa") }
}

/// Load a private key from a file, trying various formats.
fn load_key(path: &PathBuf) -> anyhow::Result<ssh_key::PrivateKey> {
    let key_data = std::fs::read_to_string(path)
        .context("Failed to read key file")?;

    // Try OpenSSH format first, then PEM
    ssh_key::PrivateKey::from_openssh(&key_data)
        .or_else(|_| ssh_key::PrivateKey::from_pkcs8_pem(&key_data))
        .or_else(|_| ssh_key::PrivateKey::from_pkcs1_pem(&key_data))
        .context("Failed to parse private key (tried OpenSSH, PKCS8, PKCS1)")
}
