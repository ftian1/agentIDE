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
    pub container: Option<String>,
    #[serde(default = "default_cols")]
    pub terminal_cols: u16,
    #[serde(default = "default_rows")]
    pub terminal_rows: u16,
}

fn default_cols() -> u16 { 80 }
fn default_rows() -> u16 { 24 }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub connection_id: String,
    pub tool: String,
    pub tool_version: Option<String>,
    pub state: String,
    pub pid: u32,
    pub created_at: String,
    // Fields with defaults — frontend expects these for session detail display
    #[serde(default = "default_cols")]
    pub cols: u16,
    #[serde(default = "default_rows")]
    pub rows: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub metadata: SessionMetadataDto,
    #[serde(default)]
    pub cost: CostBreakdownDto,
    #[serde(default)]
    pub turn_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetadataDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_repo: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CostBreakdownDto {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
    #[serde(default)]
    pub cache_write_tokens: u64,
    #[serde(default)]
    pub cost_usd: f64,
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

/// Tauri event payload for agent stream events (thought/action/observation).
#[derive(Debug, Clone, Serialize)]
pub struct AgentEventPayload {
    pub session_id: String,
    pub kind: String,
    pub text: String,
    pub code: Option<String>,
    pub label: Option<String>,
    pub status: Option<String>,
    pub seq: u64,
}

/// Tauri event payload for agent approval requests.
#[derive(Debug, Clone, Serialize)]
pub struct ApprovalRequestEvent {
    pub session_id: String,
    pub request_id: String,
    pub title: String,
    pub scope: String,
    pub command: String,
    pub cwd: Option<String>,
}

#[tauri::command]
pub async fn spawn_session(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    connection_id: String,
    req: SpawnRequest,
) -> Result<SessionInfo, String> {
    // Look up transport: prefer per-connection transport, fall back to local agent
    let transport: Arc<dyn crate::transport::Transport> =
        if let Some(t) = state.connection_transports.get(&connection_id) {
            t.value().clone()
        } else {
            let t = state.agent_transport.read().await;
            t.as_ref().cloned().ok_or("No agent connected")?
        };

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
        container: req.container.clone(),
    };

    transport.send(msg).await.map_err(|e| format!("Send failed: {}", e))?;
    tracing::info!(%session_id, %connection_id, "SpawnSession sent, waiting for ack");

    // Wait for SpawnSessionAck
    let transport_for_recv = transport.clone();
    let mut attempts = 0;
    loop {
        match transport_for_recv.recv().await.map_err(|e| format!("Recv failed: {}", e))? {
            Some(ProtocolMessage::SpawnSessionAck { session_id: sid, pid, tool_version }) => {
                tracing::info!(ack_session = %sid, expected = %session_id, "Got SpawnSessionAck");
                if sid == session_id {
                    tracing::info!(%session_id, pid, "Session spawned — starting relay task");

                    // Record session → connection mapping for write/resize/close
                    state.session_connections.insert(session_id.clone(), connection_id.clone());

                    let handle = app_handle.clone();
                    let sid_clone = session_id.clone();
                    let t_relay = transport.clone();
                    tokio::spawn(async move {
                        relay_session_output(t_relay, handle, sid_clone).await;
                    });
                    tracing::info!(%session_id, "Relay task spawned");

                    return Ok(SessionInfo {
                        id: session_id,
                        connection_id,
                        tool: req.tool,
                        tool_version,
                        state: "running".into(),
                        pid,
                        created_at: chrono::Utc::now().to_rfc3339(),
                        cols: req.terminal_cols,
                        rows: req.terminal_rows,
                        ended_at: None,
                        exit_code: None,
                        metadata: SessionMetadataDto {
                            cwd: req.cwd.clone(),
                            git_branch: None,
                            git_repo: None,
                            args: req.args.clone(),
                        },
                        cost: CostBreakdownDto::default(),
                        turn_count: 0,
                    });
                }
            }
            Some(ProtocolMessage::SpawnSessionNack { session_id: sid, reason }) => {
                tracing::warn!(nack_session = %sid, %reason, "Got SpawnSessionNack");
                if sid == session_id {
                    return Err(format!("Spawn failed: {}", reason));
                }
            }
            Some(other) => {
                if attempts < 5 {
                    tracing::debug!(kind = other.kind(), %session_id, "Ignoring message while waiting for ack");
                }
            }
            None => {
                attempts += 1;
                if attempts == 1 {
                    tracing::warn!(%session_id, "recv() returned None — transport may not be receiving");
                }
                if attempts % 100 == 0 {
                    tracing::warn!(%session_id, attempts, "Still waiting for SpawnSessionAck...");
                }
                if attempts > 600 {
                    return Err("Timeout waiting for spawn response (30s)".into());
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
    tracing::info!(%session_id, "Relay task started — listening for terminal output");

    let mut terminal_buf = String::new();
    let mut current_change_set_id: Option<String> = None;
    let mut empty_polls: u64 = 0;
    let mut msg_count: u64 = 0;
    let mut terminal_count: u64 = 0;
    let mut ignored_count: u64 = 0;

    loop {
        match transport.recv().await {
            Ok(Some(ProtocolMessage::TerminalData { session_id: sid, data, seq })) => {
                msg_count += 1;
                if sid != session_id {
                    ignored_count += 1;
                    if ignored_count <= 3 {
                        tracing::debug!(msg_sid = %sid, relay_sid = %session_id, "Relay ignoring TerminalData for different session");
                    }
                    continue;
                }
                terminal_count += 1;
                if terminal_count <= 5 || terminal_count % 50 == 0 {
                    tracing::info!(%session_id, bytes = data.len(), seq, count = terminal_count, "Relay emitting terminal:data");
                }

                match app_handle.emit("terminal:data", TerminalDataEvent {
                    session_id: sid.clone(),
                    data: data.clone(),
                    seq,
                }) {
                    Ok(()) => {}
                    Err(e) => tracing::error!(%session_id, error = %e, "Failed to emit terminal:data event"),
                }

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
                msg_count += 1;
                if sid != session_id { continue; }
                tracing::info!(%session_id, ?event_type, "Relay emitting session:event");

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
            Ok(Some(ProtocolMessage::AgentEvent { session_id: sid, kind, text, code, label, status, seq })) => {
                msg_count += 1;
                if sid != session_id { continue; }
                let _ = app_handle.emit("agent:event", AgentEventPayload {
                    session_id: sid,
                    kind: format!("{:?}", kind).to_lowercase(),
                    text, code, label, status, seq,
                });
            }
            Ok(Some(ProtocolMessage::ApprovalRequest { session_id: sid, request_id, title, scope, command, cwd })) => {
                msg_count += 1;
                if sid != session_id { continue; }
                tracing::info!(%session_id, %request_id, "Relay emitting approval:request");
                let _ = app_handle.emit("approval:request", ApprovalRequestEvent {
                    session_id: sid, request_id, title, scope, command, cwd,
                });
            }
            Ok(Some(ProtocolMessage::CloseSessionAck { .. })) => {
                tracing::info!(%session_id, "Relay received CloseSessionAck, exiting");
                break;
            }
            Ok(Some(other)) => {
                msg_count += 1;
                if msg_count <= 5 {
                    tracing::debug!(%session_id, kind = other.kind(), "Relay received other message");
                }
            }
            Ok(None) => {
                empty_polls += 1;
                if empty_polls == 1 {
                    tracing::info!(%session_id, "Relay first empty poll — transport has no data yet, will keep waiting");
                }
                if empty_polls % 200 == 0 {
                    tracing::info!(%session_id, empty_polls, msg_count, terminal_count, "Relay still alive, waiting for data");
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }
            Err(e) => {
                tracing::warn!(error = %e, %session_id, msg_count, terminal_count, empty_polls, "Transport error, relay exiting");
                break;
            }
        }
    }
    tracing::info!(%session_id, msg_count, terminal_count, "Relay task ended");
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

/// Resolve the transport for a given session by looking up its connection.
fn resolve_session_transport(
    state: &AppState,
    session_id: &str,
) -> Option<Arc<dyn crate::transport::Transport>> {
    // Look up session → connection_id
    let conn_id = state.session_connections.get(session_id)?.clone();
    // Look up connection_id → transport
    state.connection_transports.get(&conn_id).map(|r| r.value().clone())
}

#[tauri::command]
pub async fn close_session(
    state: State<'_, AppState>,
    _connection_id: String,
    session_id: String,
) -> Result<(), String> {
    let transport = resolve_session_transport(&state, &session_id)
        .or_else(|| {
            // Fallback: try to close via agent_transport (local IPC sessions)
            None
        })
        .ok_or_else(|| format!("No transport for session {}", session_id))?;

    transport
        .send(ProtocolMessage::CloseSession { session_id: session_id.clone() })
        .await
        .map_err(|e| format!("Send failed: {}", e))?;

    // Clean up session mapping
    state.session_connections.remove(&session_id);

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
    let transport = resolve_session_transport(&state, &session_id)
        .ok_or_else(|| format!("No transport for session {}", session_id))?;

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
    let transport = resolve_session_transport(&state, &session_id)
        .ok_or_else(|| format!("No transport for session {}", session_id))?;

    tracing::info!(%session_id, len = data.len(), "write_input: sending to agent");

    transport
        .send(ProtocolMessage::TerminalInput { session_id: session_id.clone(), data })
        .await
        .map_err(|e| format!("Send failed: {}", e))?;

    tracing::info!(%session_id, "write_input: sent OK");

    Ok(())
}

/// Send a chat message to the agent CLI (writes the text + newline to stdin).
#[tauri::command]
pub async fn send_agent_message(
    state: State<'_, AppState>,
    session_id: String,
    text: String,
) -> Result<(), String> {
    let transport = resolve_session_transport(&state, &session_id)
        .ok_or_else(|| format!("No transport for session {}", session_id))?;

    let mut data = text.into_bytes();
    data.push(b'\n');

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
