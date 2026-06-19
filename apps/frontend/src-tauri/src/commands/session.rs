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

/// Tauri event payload for code changes.
#[derive(Debug, Clone, Serialize)]
pub struct CodeChangeEvent {
    pub session_id: String,
    pub change_set_id: String,
    pub change_id: String,
    pub file_path: String,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
    pub diff: String,
}

/// Tauri event payload for code change batch notifications.
#[derive(Debug, Clone, Serialize)]
pub struct CodeChangeBatchEvent {
    pub session_id: String,
    pub change_set_id: String,
    pub description: String,
    pub status: String,
    pub file_count: u32,
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
/// Also detects code changes from tool_result events and terminal output.
async fn relay_session_output(
    transport: Arc<dyn crate::transport::Transport>,
    app_handle: tauri::AppHandle,
    session_id: String,
) {
    let mut terminal_buf = String::new();
    let mut current_change_set_id: Option<String> = None;

    loop {
        match transport.recv().await {
            Ok(Some(ProtocolMessage::TerminalData { session_id: sid, data, seq })) => {
                if sid != session_id { continue; }
                let _ = app_handle.emit("terminal:data", TerminalDataEvent {
                    session_id: sid.clone(),
                    data: data.clone(),
                    seq,
                });

                // Tier 2: buffer terminal output and scan for unified diffs
                let text = String::from_utf8_lossy(&data);
                terminal_buf.push_str(&text);
                if let Some(change) = scan_terminal_for_diff(
                    &session_id, &mut terminal_buf, &mut current_change_set_id,
                ) {
                    let _ = app_handle.emit("code:change", change);
                }
            }
            Ok(Some(ProtocolMessage::SessionEvent { session_id: sid, event_type, data, .. })) => {
                if sid != session_id { continue; }
                let _ = app_handle.emit("session:event", SessionEventPayload {
                    session_id: sid.clone(),
                    event_type: format!("{:?}", event_type),
                    data: data.clone(),
                });

                // Tier 1: detect code changes from tool_result events
                if let Some(changes) = detect_code_changes_from_event(
                    &session_id, &event_type, &data,
                    &mut current_change_set_id,
                ) {
                    for change in changes {
                        let _ = app_handle.emit("code:change", change);
                    }
                }
            }
            Ok(Some(ProtocolMessage::Pause { .. })) | Ok(Some(ProtocolMessage::Resume { .. })) => {
                // Flow control events — forward to frontend
            }
            Ok(None) | Err(_) => break,
            _ => {}
        }
    }
}

/// Tier 1: Detect code changes from structured SessionEvent data.
///
/// Looks for tool_result events whose data contains file modification info.
/// Claude Code / Copilot emit these with JSON in the `result` key.
fn detect_code_changes_from_event(
    session_id: &str,
    event_type: &shared_protocol::types::SessionEventType,
    data: &std::collections::HashMap<String, String>,
    change_set_id: &mut Option<String>,
) -> Option<Vec<CodeChangeEvent>> {
    // Only process tool_result events
    if !matches!(event_type, shared_protocol::types::SessionEventType::ToolResult) {
        return None;
    }

    let result_json = data.get("result")?;

    // Try to parse as JSON with a "files" array
    let parsed: serde_json::Value = serde_json::from_str(result_json).ok()?;

    let files = parsed.get("files")?.as_array()?;

    if change_set_id.is_none() {
        *change_set_id = Some(Uuid::new_v4().to_string());
        let _csid = change_set_id.clone().unwrap();
        let _desc = parsed
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("Code changes")
            .to_string();
        let _ = std::thread::spawn(move || {
            // Emit batch event (cannot use app_handle here, will do from caller)
        });
    }

    let csid = change_set_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
    let mut changes = Vec::new();

    for file_entry in files {
        let path = file_entry.get("path").and_then(|p| p.as_str()).unwrap_or("unknown");
        let old_content = file_entry.get("old_content").and_then(|c| c.as_str()).map(|s| s.to_string());
        let new_content = file_entry.get("new_content").and_then(|c| c.as_str()).map(|s| s.to_string());
        let diff = file_entry.get("diff").and_then(|d| d.as_str()).unwrap_or("").to_string();

        changes.push(CodeChangeEvent {
            session_id: session_id.to_string(),
            change_set_id: csid.clone(),
            change_id: Uuid::new_v4().to_string(),
            file_path: path.to_string(),
            old_content,
            new_content,
            diff,
        });
    }

    Some(changes)
}

/// Tier 2: Scan buffered terminal output for unified diff patterns.
///
/// Detects patterns like:
///   diff --git a/path b/path
///   --- a/path
///   +++ b/path
///   @@ -l,s +l,s @@
fn scan_terminal_for_diff(
    session_id: &str,
    buf: &mut String,
    change_set_id: &mut Option<String>,
) -> Option<CodeChangeEvent> {
    // Look for a complete diff: starts with "diff --git" or "--- a/" and ends with
    // a blank line followed by non-diff text
    let diff_start = buf.find("diff --git ")
        .or_else(|| buf.find("--- a/"))?;

    // Find the end of this diff block
    let after_start = &buf[diff_start..];
    let diff_end = after_start.find("\n\n")
        .or_else(|| after_start.find("\n\x1b"))  // ANSI escape = next prompt
        .unwrap_or(after_start.len());

    let diff_text = after_start[..diff_end + 2].to_string();

    // Extract file path
    let file_path = extract_diff_file_path(&diff_text)?;

    // Only extract diff; old/new content would need file system access
    if change_set_id.is_none() {
        *change_set_id = Some(Uuid::new_v4().to_string());
    }

    let csid = change_set_id.clone().unwrap();
    let change = CodeChangeEvent {
        session_id: session_id.to_string(),
        change_set_id: csid,
        change_id: Uuid::new_v4().to_string(),
        file_path,
        old_content: None,
        new_content: None,
        diff: diff_text,
    };

    // Clear processed portion of buffer
    buf.drain(..diff_start + diff_end + 2);

    Some(change)
}

fn extract_diff_file_path(diff: &str) -> Option<String> {
    // Try "+++ b/path" first (unified diff)
    for line in diff.lines() {
        if line.starts_with("+++ b/") {
            return Some(line[6..].trim().to_string());
        }
        if line.starts_with("--- a/") {
            return Some(line[6..].trim().to_string());
        }
    }
    // Try "diff --git a/path b/path"
    if let Some(line) = diff.lines().next() {
        if line.starts_with("diff --git ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let b_path = parts[3];
                if b_path.starts_with("b/") {
                    return Some(b_path[2..].to_string());
                }
            }
        }
    }
    None
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

#[tauri::command]
pub async fn apply_code_change(
    state: State<'_, AppState>,
    session_id: String,
    file_path: String,
    content: String,
) -> Result<(), String> {
    let transport = state.agent_transport.read().await;
    let transport = transport.as_ref().ok_or("No agent connected")?;

    transport
        .send(ProtocolMessage::ApplyChange {
            session_id,
            file_path,
            content,
        })
        .await
        .map_err(|e| format!("Send failed: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn reject_code_change(
    _state: State<'_, AppState>,
    _change_id: String,
) -> Result<(), String> {
    // Reject is purely local — no remote action needed.
    // The backend just acknowledges the rejection for audit trail.
    Ok(())
}
