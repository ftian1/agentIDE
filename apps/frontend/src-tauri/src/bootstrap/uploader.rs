//! Upload the Remote Agent Host binary to the remote Linux machine.
//!
//! Uses raw binary transfer over an SSH channel (single operation, no
//! base64 encoding, no chunking, no inter-chunk sleep).
//!
//! The agent binary is NOT embedded in the exe via include_bytes! — that
//! pattern tripped AV heuristics ("PE containing another PE" = dropper).
//! Instead, the binary is downloaded on demand from the OTA server and
//! cached locally.  The OTA background updater also pre-fetches it, so
//! the first SSH connection after startup normally finds it already cached.

use anyhow::Context;
use sha2::{Sha256, Digest};

use crate::connection::ssh::{self, SshSession};

/// OTA distribution base URL (same as updater::DIST_BASE).
const DIST_BASE: &str = "https://raw.githubusercontent.com/ftian1/agentIDE/main/dist/";

pub struct AgentBinary {
    pub arch: String,
    pub data: Vec<u8>,
    sha256: std::sync::OnceLock<String>,
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
        s
    })
}

impl AgentBinary {
    pub fn sha256_hex(&self) -> &str {
        self.sha256.get_or_init(|| {
            let mut hasher = Sha256::new();
            hasher.update(&self.data);
            bytes_to_hex(&hasher.finalize())
        })
    }
}

/// Get the agent binary for the given architecture.
///
/// 1. Try the OTA cache (populated by the background updater or a prior download).
/// 2. If missing, download from the OTA server and save to cache.
pub async fn get_agent_binary(arch: &str) -> anyhow::Result<AgentBinary> {
    let filename = format!("agent-linux-{arch}");
    let cache_path = crate::cache_dir().join(&filename);

    // 1. Try the OTA cache first.
    if cache_path.exists() {
        if let Ok(data) = std::fs::read(&cache_path) {
            tracing::info!(
                arch,
                size_kb = data.len() as f64 / 1024.0,
                "Using agent binary from cache"
            );
            return Ok(AgentBinary {
                arch: arch.to_string(),
                data,
                sha256: std::sync::OnceLock::new(),
            });
        }
        // Corrupt cache file — remove it and re-download.
        let _ = std::fs::remove_file(&cache_path);
    }

    // 2. Download from OTA server.
    let url = format!("{DIST_BASE}{filename}");
    tracing::info!(%url, arch, "Agent binary not in cache, downloading");

    let data = download_agent(&url).await
        .with_context(|| format!("Failed to download agent binary from {url}"))?;

    tracing::info!(arch, size_kb = data.len() as f64 / 1024.0, "Agent binary downloaded");

    // Save to cache for future use.
    if let Err(e) = std::fs::write(&cache_path, &data) {
        tracing::warn!(path = %cache_path.display(), error = %e, "Failed to cache agent binary");
    }

    Ok(AgentBinary {
        arch: arch.to_string(),
        data,
        sha256: std::sync::OnceLock::new(),
    })
}

// ── HTTP download (platform-specific) ────────────────────────────────

async fn download_agent(url: &str) -> anyhow::Result<Vec<u8>> {
    download_agent_inner(url).await
}

#[cfg(windows)]
mod download {
    use anyhow::Context;

    pub async fn download_agent_inner(url: &str) -> anyhow::Result<Vec<u8>> {
        let url = url.to_string();
        tokio::task::spawn_blocking(move || {
            let (status, body) = crate::commands::winhttp::get(
                &url,
                &[("User-Agent", "remote-ai-ide-bootstrap/0.1")],
            )
            .map_err(|e| anyhow::anyhow!("WinHTTP GET {url}: {e}"))?;
            if status < 200 || status >= 300 {
                anyhow::bail!("WinHTTP GET {url} returned HTTP {status}");
            }
            Ok(body)
        })
        .await
        .context("spawn_blocking join")?
    }
}

#[cfg(not(windows))]
mod download {
    use anyhow::Context;

    pub async fn download_agent_inner(url: &str) -> anyhow::Result<Vec<u8>> {
        let client = build_reqwest_client(120)?; // 120 s timeout for ~4 MB binary
        let resp = client
            .get(url)
            .header("User-Agent", "remote-ai-ide-bootstrap/0.1")
            .send()
            .await
            .context("reqwest GET")?;

        if !resp.status().is_success() {
            anyhow::bail!("GET {url} returned HTTP {}", resp.status());
        }

        resp.bytes()
            .await
            .context("reading response body")
            .map(|b| b.to_vec())
    }

    fn build_reqwest_client(timeout_secs: u64) -> anyhow::Result<reqwest::Client> {
        let mut builder = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(timeout_secs))
            .timeout(std::time::Duration::from_secs(timeout_secs));

        // Detect proxy from env vars (same as updater.rs).
        for key in &["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"] {
            if let Ok(val) = std::env::var(key) {
                let val = val.trim().to_string();
                if !val.is_empty() {
                    if let Ok(p) = reqwest::Proxy::all(&val) {
                        tracing::debug!(%val, "agent download: using proxy");
                        builder = builder.proxy(p);
                        builder = builder.danger_accept_invalid_certs(true);
                    }
                    break;
                }
            }
        }

        Ok(builder.build()?)
    }
}

use download::download_agent_inner;

/// Upload the agent binary to the remote host.
///
/// Opens a single SSH exec channel running `cat > <path>`, writes raw
/// bytes via [`ssh::upload_raw`], verifies the file size, and chmods.
///
/// Round-trips are minimized: the destination dir is created as part of the
/// upload command's shell (`mkdir -p ... && cat > tmp`), and size-verify +
/// atomic-rename + chmod are fused into a single exec call afterward. With
/// Nagle disabled on the SSH socket, the raw write itself is ~1 RTT.
pub async fn upload_agent(
    session: &SshSession,
    binary: &AgentBinary,
    home_dir: &str,
) -> anyhow::Result<String> {
    let dir = format!("{}/.remote-agent-host", home_dir.trim_end_matches('/'));
    let remote_path = format!("{}/agent", dir);
    let tmp_path = format!("{}/agent.tmp", dir);
    let expected = binary.data.len();
    tracing::info!(arch = binary.arch, size_kb = expected as f64 / 1024.0, "Uploading agent (raw)");

    // Upload to a temp path; the upload command's shell creates the dir first,
    // so this fuses mkdir + write into one channel.
    let written = ssh::upload_raw_cmd(
        session,
        &binary.data,
        &format!("mkdir -p {dir} && cat > {tmp_path}"),
    )
    .await
    .context("raw upload")?;

    if written != expected {
        anyhow::bail!("Upload size mismatch: expected {expected}, wrote {written}");
    }

    // Fuse verify + atomic rename + chmod into a single round-trip.
    // Echoes OK only if the on-disk size matches; the rename is atomic so a
    // partial upload never clobbers a good binary.
    let verify = ssh::exec_remote(session, &format!(
        "s=$(stat -c%s {tmp_path}); \
         if [ \"$s\" = \"{expected}\" ]; then \
            mv -f {tmp_path} {remote_path} && chmod +x {remote_path} && echo OK; \
         else echo \"SIZE=$s\"; fi"
    ))
    .await
    .context("verify+install")?;

    if verify.trim() != "OK" {
        anyhow::bail!("Upload verification failed: {}", verify.trim());
    }

    tracing::info!(path = %remote_path, bytes = expected, "Upload complete");
    Ok(remote_path)
}

pub async fn start_agent(
    session: &SshSession,
    home_dir: &str,
) -> anyhow::Result<std::sync::Arc<crate::transport::ssh_channel::SshChannelTransport>> {
    let path = format!("{}/.remote-agent-host/agent", home_dir.trim_end_matches('/'));
    let log_path = format!("{}/.remote-agent-host/agent.log", home_dir.trim_end_matches('/'));
    let cmd = format!("{} --mode stdio --log-level debug --log-file {}", path, log_path);
    let ch = ssh::open_exec_channel(session, &cmd).await
        .context("start agent")?;
    Ok(std::sync::Arc::new(crate::transport::ssh_channel::SshChannelTransport::new(ch)))
}
