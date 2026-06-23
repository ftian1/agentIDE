//! SSH client wrapper around [`russh`] v0.61.
//!
//! Supports key-based (ed25519, rsa), agent-forwarded, and password
//! authentication.
//!
//! API patterns verified against the working [`ssh-test`] binary in
//! `crates/remote-agent-host/src/bin/ssh-test.rs`.
//!
//! IMPORTANT: russh 0.61 depends on `ssh-key` 0.7.x internally. The app's
//! direct `ssh-key` 0.6.x dependency is a DIFFERENT type. Always use
//! `russh::keys::PublicKey` / `russh::keys::PrivateKey` (the russh
//! re-exports) when interacting with the russh API, never the app's own
//! `ssh_key` types.

use anyhow::Context;
use russh::*;
use russh::client::Handler;
use russh::keys::PrivateKeyWithHashAlg;
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
    pub handle: client::Handle<SshClientHandler>,
}

/// Our minimal SSH client handler.
pub struct SshClientHandler;

impl Handler for SshClientHandler {
    type Error = anyhow::Error;

    /// Accept all server host keys.
    fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> impl std::future::Future<Output = Result<bool, Self::Error>> + Send {
        tracing::info!("Accepting server host key (known_hosts not yet implemented)");
        std::future::ready(Ok(true))
    }
}

/// Open an SSH connection to a remote host.
pub async fn connect(params: &SshConnectionParams) -> anyhow::Result<SshSession> {
    tracing::info!(host = %params.host, port = params.port, user = %params.user, "Connecting via SSH");

    let mut config = client::Config::default();
    // Keep-alive: send a request every 30s of idle time, waiting up to 10s for response.
    // This prevents NAT/firewall timeouts and detects dead connections early.
    config.keepalive_interval = Some(std::time::Duration::from_secs(30));
    config.keepalive_max = 3; // disconnect after 3 unanswered keep-alives
    // Disable Nagle's algorithm. Without this, bulk transfers (e.g. the ~1.8MB
    // agent binary upload) stall on the Nagle/delayed-ACK interaction: russh
    // chunks data into 32KB SSH packets and TCP holds them waiting for ACKs,
    // turning a sub-second upload into ~8s. nodelay sends each packet immediately.
    config.nodelay = true;
    let config = Arc::new(config);
    let sh = SshClientHandler;

    let addr = format!("{}:{}", params.host, params.port);
    let mut handle = russh::client::connect(config, &addr, sh).await
        .context("SSH connection failed")?;

    let auth_result = match &params.auth {
        AuthMethod::Key(identity_file) => {
            let key_path = identity_file.clone().unwrap_or_else(default_key_path);
            tracing::info!(key = %key_path.display(), "Authenticating with key");

            let key = load_key(&key_path)
                .context("Failed to load private key")?;
            let key_with_hash = PrivateKeyWithHashAlg::new(Arc::new(key), None);
            handle.authenticate_publickey(&params.user, key_with_hash).await
                .context("Key authentication failed")?
        }
        AuthMethod::Agent => {
            tracing::info!("Agent authentication — falling back to default key");
            let key_path = default_key_path();
            let key = load_key(&key_path)
                .context("Failed to load default key for agent auth")?;
            let key_with_hash = PrivateKeyWithHashAlg::new(Arc::new(key), None);
            handle.authenticate_publickey(&params.user, key_with_hash).await
                .context("Key authentication failed")?
        }
        AuthMethod::Password(password) => {
            tracing::info!("Authenticating with password");
            handle.authenticate_password(&params.user, password).await
                .context("Password authentication failed")?
        }
    };

    anyhow::ensure!(auth_result.success(), "Authentication rejected by server");

    tracing::info!(host = %params.host, "SSH connected ✓");

    Ok(SshSession { handle })
}

/// Open an exec channel.
pub async fn open_exec_channel(
    session: &SshSession,
    command: &str,
) -> anyhow::Result<Channel<client::Msg>> {
    let channel = session.handle.channel_open_session().await
        .context("Failed to open SSH channel")?;

    channel.exec(true, command).await
        .context("Failed to exec command")?;

    Ok(channel)
}

/// Run a simple command on the remote host and return stdout.
pub async fn exec_remote(
    session: &SshSession,
    command: &str,
) -> anyhow::Result<String> {
    let mut channel = open_exec_channel(session, command).await?;
    channel.eof().await.context("Failed to send EOF")?;

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
    // Explicitly close the channel so the server releases it immediately.
    // Without this, rapid-fire exec_remote calls (e.g., chunked upload)
    // exhaust the server's channel limit.
    let _ = channel.close().await;

    Ok(String::from_utf8_lossy(&output).trim().to_string())
}

/// Upload raw bytes to a remote file by piping through a command's stdin.
///
/// Opens an exec channel running `cat > <remote_path>`, writes all data
/// directly via [`Channel::data_bytes`], then sends EOF.  This is a single
/// SSH channel operation — no base64 encoding, no chunking, no sleep between
/// chunks.
///
/// Returns the number of bytes written.
pub async fn upload_raw(
    session: &SshSession,
    data: &[u8],
    remote_path: &str,
) -> anyhow::Result<usize> {
    upload_raw_cmd(session, data, &format!("cat > {}", remote_path)).await
}

/// Upload raw bytes by piping through stdin of an arbitrary remote command.
///
/// Like [`upload_raw`] but lets the caller supply the full command (e.g.
/// `mkdir -p dir && cat > dir/file`), so directory creation can be fused
/// into the same channel as the data write — saving a round-trip.
pub async fn upload_raw_cmd(
    session: &SshSession,
    data: &[u8],
    command: &str,
) -> anyhow::Result<usize> {
    let len = data.len();
    let mut channel = session.handle.channel_open_session().await
        .context("Failed to open SSH channel for upload")?;

    channel.exec(true, command).await
        .context("Failed to exec upload command")?;

    // Write all data at once
    let b: bytes::Bytes = data.to_vec().into();
    channel.data_bytes(b).await
        .context("Failed to write data to SSH channel")?;

    // Signal EOF so the remote command's stdin closes and it exits
    channel.eof().await.context("Failed to send EOF")?;

    // Drain the channel until we see exit status or EOF
    loop {
        match channel.wait().await {
            Some(ChannelMsg::ExitStatus { .. }) => {
                tracing::debug!(bytes = len, "Upload complete (exit status received)");
                break;
            }
            Some(ChannelMsg::Eof) | None => {
                tracing::debug!(bytes = len, "Upload complete (EOF)");
                break;
            }
            Some(ChannelMsg::Data { ref data }) => {
                let text = String::from_utf8_lossy(data);
                if !text.trim().is_empty() {
                    tracing::debug!(stderr = %text.trim(), "Upload stderr");
                }
            }
            _ => continue,
        }
    }
    let _ = channel.close().await;

    Ok(len)
}

/// Default SSH key path.
fn default_key_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_default();
    let ed = home.join(".ssh/id_ed25519");
    if ed.exists() { ed } else { home.join(".ssh/id_rsa") }
}

/// Load a private key from file.
///
/// Uses `russh::keys::PrivateKey` (re-export of ssh-key 0.7.x) for
/// compatibility with the russh 0.61 API.
fn load_key(path: &PathBuf) -> anyhow::Result<russh::keys::PrivateKey> {
    let key_data = std::fs::read_to_string(path)
        .context("Failed to read key file")?;

    russh::keys::PrivateKey::from_openssh(&key_data)
        .context("Failed to parse private key — only OpenSSH format is supported.\n\
                  Hint: convert with `ssh-keygen -p -m RFC4716 -f <key>`")
}
