//! Bootstrap Manager — orchestrates the full remote host deployment pipeline.
//!
//! Flow: detect platform → check existing version → upload if needed → start agent → handshake.

use tauri::{AppHandle, Emitter};

use super::detector;
use super::uploader::{self, get_embedded};
use crate::connection::ssh::SshSession;

/// Phases of the bootstrap process emitted as Tauri events.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BootstrapProgressEvent {
    pub connection_id: String,
    pub phase: String,
    pub progress: f32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Run the full bootstrap pipeline for a newly connected SSH session.
///
/// Returns the transport channel receiver once the agent is started
/// and the handshake is complete.
pub async fn run_bootstrap(
    app_handle: &AppHandle,
    connection_id: &str,
    session: &SshSession,
) -> anyhow::Result<std::sync::Arc<crate::transport::ssh_channel::SshChannelTransport>> {
    let emit = |phase: &str, progress: f32, message: &str, error: Option<String>| {
        let _ = app_handle.emit("bootstrap:progress", BootstrapProgressEvent {
            connection_id: connection_id.to_string(),
            phase: phase.to_string(),
            progress,
            message: message.to_string(),
            error,
        });
    };

    // Step 1: Detect remote platform
    emit("detecting", 0.1, "Detecting remote platform...", None);
    let info = detector::detect(session).await
        .map_err(|e| {
            emit("detecting", 0.1, "Detection failed", Some(e.to_string()));
            e
        })?;

    let arch = &info.arch;
    emit("detecting", 0.3, &format!("Detected: {} {}", info.platform, arch), None);

    // Step 2: Check if we need to upload — compare embedded SHA256 with remote SHA256
    let binary = get_embedded(arch)
        .ok_or_else(|| anyhow::anyhow!("Unsupported architecture: {}", arch))?;
    let embedded_hash = binary.sha256_hex();

    let need_upload = if info.agent_sha256.is_empty() {
        tracing::info!("Agent not installed on remote, uploading");
        true
    } else if info.agent_sha256 == embedded_hash {
        tracing::info!(remote_sha256 = %&info.agent_sha256[..16], embedded_sha256 = %&embedded_hash[..16], "Agent SHA256 matches, skipping upload");
        false
    } else {
        tracing::info!(remote_sha256 = %&info.agent_sha256[..16], embedded_sha256 = %&embedded_hash[..16], "Agent SHA256 mismatch, re-uploading");
        true
    };

    if need_upload {
        emit("uploading", 0.4, "Preparing agent binary...", None);

        emit("uploading", 0.6, &format!("Uploading agent ({:.1} KB, sha256={}...)...", binary.data.len() as f64 / 1024.0, &embedded_hash[..16]), None);
        uploader::upload_agent(session, &binary, &info.home_dir).await
            .map_err(|e| {
                emit("uploading", 0.6, "Upload failed", Some(e.to_string()));
                e
            })?;

        emit("uploading", 0.8, "Agent binary installed", None);
    } else {
        emit("uploading", 0.8, "Agent already installed, skipping upload", None);
    }

    // Step 3: Start the agent (use absolute path from detection)
    emit("starting", 0.85, "Starting agent on remote host...", None);
    let transport = uploader::start_agent(session, &info.home_dir).await
        .map_err(|e| {
            emit("starting", 0.85, "Failed to start agent", Some(e.to_string()));
            e
        })?;

    emit("starting", 0.95, "Agent started", None);

    // Step 4: Handshake is handled by the main event loop — the first message
    //         from the agent should be a Hello.
    emit("complete", 1.0, "Connected", None);

    tracing::info!(connection_id = %connection_id, "Bootstrap complete");

    Ok(transport)
}
