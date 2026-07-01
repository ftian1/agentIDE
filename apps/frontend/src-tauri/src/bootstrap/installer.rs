//! Bootstrap Manager — orchestrates the full remote host deployment pipeline.
//!
//! Flow: detect platform → check existing version → upload if needed → start agent → handshake.

use tauri::{AppHandle, Emitter};

use super::detector;
use super::uploader::{self, get_agent_binary};
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

    // Step 2: Get agent binary (from cache, or download from OTA server)
    let binary = get_agent_binary(arch).await
        .map_err(|e| anyhow::anyhow!("Cannot get agent binary for {}: {}", arch, e))?;
    let embedded_hash = binary.sha256_hex();

    // Show full hashes in BOTH the bootstrap progress UI AND the agent output.
    let exe_hash_msg = format!("[bootstrap] Local agent  SHA256: {}", embedded_hash);
    let remote_hash_msg = format!("[bootstrap] Remote agent SHA256: {}", info.agent_sha256);
    emit("detecting", 0.3, &exe_hash_msg, None);
    emit("detecting", 0.3, &remote_hash_msg, None);
    crate::backend_log!(app_handle, "{}", exe_hash_msg);
    crate::backend_log!(app_handle, "{}", remote_hash_msg);

    let need_upload = if info.agent_sha256.is_empty() {
        tracing::info!(embedded_sha256 = %embedded_hash, "Agent not installed on remote, uploading");
        let msg = "[bootstrap] Agent not on remote, will upload".to_string();
        emit("uploading", 0.4, &msg, None);
        crate::backend_log!(app_handle, "{}", msg);
        true
    } else if info.agent_sha256 == embedded_hash {
        tracing::info!(remote = %info.agent_sha256, embedded = %embedded_hash, "Agent SHA256 matches, skipping upload");
        let msg = format!("[bootstrap] SHA256 match — skip upload ({})", &embedded_hash[..16]);
        emit("uploading", 0.6, &msg, None);
        crate::backend_log!(app_handle, "{}", msg);
        false
    } else {
        tracing::warn!(remote = %info.agent_sha256, embedded = %embedded_hash, "Agent SHA256 MISMATCH, re-uploading");
        let msg = format!("[bootstrap] SHA256 MISMATCH! exe={} remote={} — re-uploading", &embedded_hash[..16], &info.agent_sha256[..16]);
        emit("uploading", 0.4, &msg, None);
        crate::backend_log!(app_handle, "{}", msg);
        true
    };

    if need_upload {
        let msg1 = format!("[bootstrap] Uploading agent ({:.1} KB)...", binary.data.len() as f64 / 1024.0);
        emit("uploading", 0.5, &msg1, None);
        crate::backend_log!(app_handle, "{}", msg1);

        uploader::upload_agent(session, &binary, &info.home_dir).await
            .map_err(|e| {
                let err_msg = format!("[bootstrap] Upload FAILED: {}", e);
                emit("uploading", 0.6, &err_msg, Some(e.to_string()));
                crate::backend_log!(app_handle, "{}", err_msg);
                e
            })?;

        let done_msg = "[bootstrap] Agent binary installed".to_string();
        emit("uploading", 0.8, &done_msg, None);
        crate::backend_log!(app_handle, "{}", done_msg);
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
