//! Upload the Remote Agent Host binary to the remote Linux machine.
//!
//! The binary is embedded at compile time. We upload it by:
//! 1. Creating the target directory (ssh exec: mkdir -p)
//! 2. Base64-encoding the binary and piping it via ssh exec to
//!    `base64 -d > ~/.remote-agent-host/agent`
//! 3. Setting executable permissions (ssh exec: chmod +x)

use anyhow::Context;
use base64::Engine;

use crate::connection::ssh::{self, SshSession};
use crate::transport::Transport;

/// Architecture-specific binary data.
pub struct EmbeddedBinary {
    pub arch: &'static str,
    pub data: &'static [u8],
}

/// Return the embedded binary for the given architecture, if available.
pub fn get_embedded(arch: &str) -> Option<EmbeddedBinary> {
    match arch {
        "x86_64" => Some(EmbeddedBinary {
            arch: "x86_64",
            data: include_bytes!("../../../binaries/remote-agent-host-x86_64"),
        }),
        "aarch64" => Some(EmbeddedBinary {
            arch: "aarch64",
            data: include_bytes!("../../../binaries/remote-agent-host-aarch64"),
        }),
        _ => None,
    }
}

/// Upload the agent binary to the remote host and make it executable.
pub async fn upload_agent(
    session: &SshSession,
    binary: &EmbeddedBinary,
) -> anyhow::Result<String> {
    let remote_path = "~/.remote-agent-host/agent";
    let size_kb = binary.data.len() as f64 / 1024.0;

    tracing::info!(arch = binary.arch, size_kb, "Uploading agent binary");

    // Create the target directory
    ssh::exec_remote(session, "mkdir -p ~/.remote-agent-host/").await
        .context("Failed to create target directory")?;

    // Encode binary as base64 and pipe to remote
    let b64 = base64::engine::general_purpose::STANDARD.encode(binary.data);

    // Split into chunks to avoid overwhelming the SSH channel buffer
    const CHUNK_SIZE: usize = 65536; // 64KB base64 chunks
    let total_chunks = (b64.len() + CHUNK_SIZE - 1) / CHUNK_SIZE;

    // Write the file in chunks: first chunk creates/overwrites, subsequent chunks append
    for (i, chunk) in b64.as_bytes().chunks(CHUNK_SIZE).enumerate() {
        let chunk_str = String::from_utf8_lossy(chunk);
        let cmd = if i == 0 {
            format!("echo '{}' | base64 -d > {}", chunk_str, remote_path)
        } else {
            format!("echo '{}' | base64 -d >> {}", chunk_str, remote_path)
        };

        ssh::exec_remote(session, &cmd).await
            .context(format!("Failed to upload chunk {}/{}", i + 1, total_chunks))?;

        if total_chunks > 1 && (i + 1) % 10 == 0 {
            tracing::debug!("Upload progress: {}/{} chunks", i + 1, total_chunks);
        }
    }

    // Set executable permissions
    ssh::exec_remote(session, "chmod +x ~/.remote-agent-host/agent").await
        .context("Failed to chmod agent binary")?;

    tracing::info!(path = remote_path, "Agent binary uploaded and ready");

    Ok(remote_path.to_string())
}

/// Start the agent on the remote host via SSH exec.
/// Spawns a background task that reads messages from the SSH channel
/// and forwards them to the provided sender.
pub async fn start_agent(
    session: &SshSession,
    transport_tx: tokio::sync::mpsc::UnboundedSender<shared_protocol::ProtocolMessage>,
) -> anyhow::Result<()> {
    let channel = ssh::open_exec_channel(
        session,
        "~/.remote-agent-host/agent --mode stdio --log-level info",
    )
    .await
    .context("Failed to start agent on remote host")?;

    let transport = crate::transport::ssh_channel::SshChannelTransport::new(channel);

    // Spawn reader task
    tokio::spawn(async move {
        loop {
            match transport.recv().await {
                Ok(Some(msg)) => {
                    tracing::trace!(kind = msg.kind(), "Received from agent");
                    if transport_tx.send(msg).is_err() {
                        tracing::warn!("Transport channel closed");
                        break;
                    }
                }
                Ok(None) => {
                    tracing::info!("Agent transport closed (EOF)");
                    break;
                }
                Err(e) => {
                    tracing::error!(error = %e, "Agent transport error");
                    break;
                }
            }
        }
    });

    Ok(())
}
