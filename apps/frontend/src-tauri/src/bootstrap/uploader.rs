//! Upload the Remote Agent Host binary to the remote Linux machine.
//!
//! Uses raw binary transfer over an SSH channel (single operation, no
//! base64 encoding, no chunking, no inter-chunk sleep).

use anyhow::Context;
use sha2::{Sha256, Digest};

use crate::connection::ssh::{self, SshSession};

pub struct EmbeddedBinary {
    pub arch: &'static str,
    pub data: &'static [u8],
    /// Pre-computed SHA256 hex string (computed once at first access).
    sha256: std::sync::OnceLock<String>,
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
        s
    })
}

impl EmbeddedBinary {
    pub fn sha256_hex(&self) -> &str {
        self.sha256.get_or_init(|| {
            let mut hasher = Sha256::new();
            hasher.update(self.data);
            bytes_to_hex(&hasher.finalize())
        })
    }
}

pub fn get_embedded(arch: &str) -> Option<EmbeddedBinary> {
    match arch {
        "x86_64" => Some(EmbeddedBinary {
            arch: "x86_64",
            data: include_bytes!("../../binaries/remote-agent-host-x86_64"),
            sha256: std::sync::OnceLock::new(),
        }),
        "aarch64" => Some(EmbeddedBinary {
            arch: "aarch64",
            data: include_bytes!("../../binaries/remote-agent-host-aarch64"),
            sha256: std::sync::OnceLock::new(),
        }),
        _ => None,
    }
}

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
    binary: &EmbeddedBinary,
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
        binary.data,
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
