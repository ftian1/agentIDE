//! Tauri IPC commands for session lifecycle.
//!
//! These forward requests to the Remote Agent Host via the active transport
//! and emit Tauri events for streaming responses (TerminalData, SessionEvent).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};
use uuid::Uuid;

use shared_protocol::messages::ProtocolMessage;
use shared_protocol::types::ToolKind;

use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnRequest {
    pub tool: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    #[serde(default = "default_cols")]
    pub terminal_cols: u16,
    #[serde(default = "default_rows")]
    pub terminal_rows: u16,
}

fn default_cols() -> u16 { 80 }
fn default_rows() -> u16 { 24 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub connection_id: String,
    pub tool: String,
    pub tool_version: Option<String>,
    pub state: String,
    pub pid: u32,
    pub created_at: String,
}

/// Tauri event payload for terminal data streams.
#[derive(Debug, Clone, Serialize)]
pub struct TerminalDataEvent {
    pub session_id: String,
    pub data: Vec<u8>,
    pub seq: u64,
}

/// Tauri event payload for session events.
#[derive(Debug, Clone, Serialize)]
pub struct SessionEventPayload {
    pub session_id: String,
    pub event_type: String,
    pub data: std::collections::HashMap<String, String>,
}

#[tauri::command]
pub async fn spawn_session(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    connection_id: String,
    req: SpawnRequest,
) -> Result<SessionInfo, String> {
    let transport = state.agent_transport.read().await;
    let transport = transport.as_ref().ok_or("No agent connected")?;

    let session_id = Uuid::new_v4().to_string();
    let tool = match req.tool.as_str() {
        "claude" => ToolKind::Claude,
        "copilot" => ToolKind::Copilot,
        other => ToolKind::Custom(other.to_string()),
    };

    let msg = ProtocolMessage::SpawnSession {
        session_id: session_id.clone(),
        tool,
        args: req.args.clone(),
        env: req.env.unwrap_or_default(),
        cwd: req.cwd.clone(),
        terminal_cols: req.terminal_cols,
        terminal_rows: req.terminal_rows,
    };

    transport.send(msg).await.map_err(|e| format!("Send failed: {}", e))?;

    // Wait for SpawnSessionAck (with a simple polling approach)
    let mut attempts = 0;
    loop {
        match transport.recv().await.map_err(|e| format!("Recv failed: {}", e))? {
            Some(ProtocolMessage::SpawnSessionAck { session_id: sid, pid, tool_version }) => {
                if sid == session_id {
                    tracing::info!(%session_id, pid, "Session spawned");

                    // Start a background task to relay terminal data for this session
                    let transport_clone = state.agent_transport.read().await;
                    if let Some(t) = transport_clone.as_ref().cloned() {
                        let handle = app_handle.clone();
                        let sid_clone = session_id.clone();
                        tokio::spawn(async move {
                            relay_session_output(t, handle, sid_clone).await;
                        });
                    }

                    return Ok(SessionInfo {
                        id: session_id,
                        connection_id,
                        tool: req.tool,
                        tool_version,
                        state: "running".into(),
                        pid,
                        created_at: chrono::Utc::now().to_rfc3339(),
                    });
                }
            }
            Some(ProtocolMessage::SpawnSessionNack { session_id: sid, reason }) => {
                if sid == session_id {
                    return Err(format!("Spawn failed: {}", reason));
                }
            }
            Some(_) => {} // Ignore other messages
            None => {
                attempts += 1;
                if attempts > 100 {
                    return Err("Timeout waiting for spawn response".into());
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }
        }
    }
}

/// Relay terminal data and session events from the agent to the frontend.
async fn relay_session_output(
    transport: Arc<dyn crate::transport::Transport>,
    app_handle: tauri::AppHandle,
    session_id: String,
) {
    loop {
        match transport.recv().await {
            Ok(Some(ProtocolMessage::TerminalData { session_id: sid, data, seq })) => {
                if sid != session_id { continue; }
                let _ = app_handle.emit("terminal:data", TerminalDataEvent {
                    session_id: sid,
                    data,
                    seq,
                });
            }
            Ok(Some(ProtocolMessage::SessionEvent { session_id: sid, event_type, data, .. })) => {
                if sid != session_id { continue; }
                let _ = app_handle.emit("session:event", SessionEventPayload {
                    session_id: sid,
                    event_type: format!("{:?}", event_type),
                    data,
                });
            }
            Ok(Some(ProtocolMessage::Pause { .. })) | Ok(Some(ProtocolMessage::Resume { .. })) => {
                // Flow control events — forward to frontend
            }
            Ok(None) | Err(_) => break,
            _ => {}
        }
    }
}

#[tauri::command]
pub async fn close_session(
    state: State<'_, AppState>,
    _connection_id: String,
    session_id: String,
) -> Result<(), String> {
    let transport = state.agent_transport.read().await;
    let transport = transport.as_ref().ok_or("No agent connected")?;

    transport
        .send(ProtocolMessage::CloseSession { session_id })
        .await
        .map_err(|e| format!("Send failed: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn resize_terminal(
    state: State<'_, AppState>,
    _connection_id: String,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let transport = state.agent_transport.read().await;
    let transport = transport.as_ref().ok_or("No agent connected")?;

    transport
        .send(ProtocolMessage::TerminalResize { session_id, cols, rows })
        .await
        .map_err(|e| format!("Send failed: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn write_input(
    state: State<'_, AppState>,
    _connection_id: String,
    session_id: String,
    data: Vec<u8>,
) -> Result<(), String> {
    let transport = state.agent_transport.read().await;
    let transport = transport.as_ref().ok_or("No agent connected")?;

    transport
        .send(ProtocolMessage::TerminalInput { session_id, data })
        .await
        .map_err(|e| format!("Send failed: {}", e))?;

    Ok(())
}
