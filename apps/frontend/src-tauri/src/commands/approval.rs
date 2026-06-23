//! Tauri IPC command for responding to agent approval requests.
//!
//! Forwards the user's decision to the Remote Agent Host as an
//! `ApprovalResponse` over the session's transport.

use std::sync::Arc;
use tauri::State;

use shared_protocol::messages::ProtocolMessage;
use shared_protocol::types::ApprovalDecision;

use crate::AppState;

/// Respond to a pending agent approval request.
#[tauri::command]
pub async fn respond_approval(
    state: State<'_, AppState>,
    session_id: String,
    request_id: String,
    decision: String,
) -> Result<(), String> {
    let decision = match decision.as_str() {
        "allow" => ApprovalDecision::Allow,
        "allowAll" => ApprovalDecision::AllowAll,
        "reject" => ApprovalDecision::Reject,
        other => return Err(format!("unknown decision: {other}")),
    };

    let conn_id = state
        .session_connections
        .get(&session_id)
        .map(|r| r.value().clone())
        .ok_or_else(|| format!("No connection for session {session_id}"))?;

    let transport: Arc<dyn crate::transport::Transport> = state
        .connection_transports
        .get(&conn_id)
        .map(|r| r.value().clone())
        .ok_or_else(|| format!("No transport for connection {conn_id}"))?;

    transport
        .send(ProtocolMessage::ApprovalResponse {
            session_id,
            request_id,
            decision,
        })
        .await
        .map_err(|e| format!("Send failed: {e}"))
}
