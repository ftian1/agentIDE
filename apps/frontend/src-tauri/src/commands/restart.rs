//! Restart command — saves application state before a graceful restart
//! (for OTA updates) so that SSH connections, sessions, and conversation
//! history can be recovered after the process restarts.
//!
//! Distinguishes between "restart for update" (save everything) and
//! "user intentionally close/disconnect" (clean shutdown).

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

/// Saved in `cache/.restart_pending` — read on next startup to know
/// this was a graceful restart and which connections to auto-reconnect.
#[derive(Serialize, Deserialize)]
struct RestartFlag {
    reason: String,              // "ota_update"
    timestamp: String,           // ISO 8601
    active_connections: Vec<ActiveConnection>,
    active_sessions: Vec<ActiveSession>,
}

#[derive(Serialize, Deserialize, Clone)]
struct ActiveConnection {
    id: String,
    label: String,
    host: String,
    port: u16,
    user: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct ActiveSession {
    session_id: String,
    connection_id: String,
    working_dir: Option<String>,
    agent_pid: Option<u32>,
}

/// Called by the frontend when the user clicks "Restart to update".
/// Saves all active connections and sessions to a flag file, then
/// exits. On next startup, the setup closure reads the flag and
/// auto-reconnects.
#[tauri::command]
pub async fn prepare_restart(
    state: State<'_, AppState>,
) -> Result<(), String> {
    let cache = crate::cache_dir();
    std::fs::create_dir_all(&cache).map_err(|e| format!("create cache: {e}"))?;

    tracing::info!("restart: preparing graceful restart...");

    // Collect active connections.
    let mut active_connections = Vec::new();
    for entry in state.connections.connections.iter() {
        let conn = entry.value().clone();
        active_connections.push(ActiveConnection {
            id: conn.id.clone(),
            label: conn.label.clone(),
            host: conn.host.clone(),
            port: conn.port,
            user: conn.user.clone(),
        });
    }
    tracing::info!(
        "restart: saving {} active connection(s)",
        active_connections.len()
    );

    // Collect active sessions.
    let mut active_sessions = Vec::new();
    for entry in state.session_connections.iter() {
        active_sessions.push(ActiveSession {
            session_id: entry.key().clone(),
            connection_id: entry.value().clone(),
            working_dir: None,
            agent_pid: None,
        });
    }
    tracing::info!(
        "restart: saving {} active session(s)",
        active_sessions.len()
    );

    let flag = RestartFlag {
        reason: "ota_update".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        active_connections,
        active_sessions,
    };

    let flag_path = cache.join(".restart_pending");
    let json = serde_json::to_string_pretty(&flag)
        .map_err(|e| format!("serialize flag: {e}"))?;
    std::fs::write(&flag_path, &json)
        .map_err(|e| format!("write flag: {e}"))?;

    tracing::info!(
        "restart: flag written to {} — restarting",
        flag_path.display()
    );

    tracing::info!("restart: flag written — exiting process");
    std::process::exit(0);
}

/// Query whether a graceful restart is pending.  Called by the frontend
/// on boot to decide whether to show "recovering from restart" UI.
#[tauri::command]
pub fn check_restart_flag() -> Result<bool, String> {
    let flag_path = crate::cache_dir().join(".restart_pending");
    Ok(flag_path.exists())
}
