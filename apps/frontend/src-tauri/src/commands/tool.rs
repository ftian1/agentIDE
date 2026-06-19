//! Tauri IPC commands for tool probing and installation.

use serde::{Deserialize, Serialize};
use tauri::State;
use shared_protocol::messages::ProtocolMessage;
use shared_protocol::types::ToolKind;
use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub tool: String,
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub auth_ok: Option<bool>,
    pub details: Option<std::collections::HashMap<String, String>>,
}

#[tauri::command]
pub async fn probe_tool(
    state: State<'_, AppState>,
    _connection_id: String,
    tool: String,
) -> Result<ProbeResult, String> {
    let transport = state.agent_transport.read().await;
    let transport = transport.as_ref().ok_or("No agent connected")?;

    let tool_kind = match tool.as_str() {
        "claude" => ToolKind::Claude,
        "copilot" => ToolKind::Copilot,
        other => ToolKind::Custom(other.into()),
    };

    transport.send(ProtocolMessage::ProbeRequest { tool: tool_kind })
        .await.map_err(|e| format!("Send: {}", e))?;

    // Wait for ProbeResponse
    for _ in 0..30 {
        match transport.recv().await.map_err(|e| format!("Recv: {}", e))? {
            Some(ProtocolMessage::ProbeResponse { tool: t, installed, version, path, auth_ok, details }) => {
                return Ok(ProbeResult {
                    tool: format!("{:?}", t),
                    installed,
                    version,
                    path,
                    auth_ok,
                    details,
                });
            }
            Some(_) => {}
            None => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
        }
    }
    Err("Probe timeout".into())
}

#[tauri::command]
pub async fn install_tool(
    state: State<'_, AppState>,
    _connection_id: String,
    tool: String,
    version: Option<String>,
) -> Result<(), String> {
    let transport = state.agent_transport.read().await;
    let transport = transport.as_ref().ok_or("No agent connected")?;

    let tool_kind = match tool.as_str() {
        "claude" => ToolKind::Claude,
        "copilot" => ToolKind::Copilot,
        other => ToolKind::Custom(other.into()),
    };

    transport.send(ProtocolMessage::InstallRequest { tool: tool_kind, version })
        .await.map_err(|e| format!("Send: {}", e))?;

    // Progress events arrive via Tauri events, not here
    Ok(())
}
