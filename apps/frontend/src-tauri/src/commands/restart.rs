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
    prepare_restart_inner(state).await?;
    tracing::info!("restart: flag written — exiting process");
    std::process::exit(0);
}

/// Save the restart flag without exiting. Used by both `prepare_restart`
/// and `apply_update_and_restart`.
async fn prepare_restart_inner(
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
        "restart: flag written to {}",
        flag_path.display()
    );

    Ok(())
}

/// Query whether a graceful restart is pending.  Called by the frontend
/// on boot to decide whether to show "recovering from restart" UI.
#[tauri::command]
pub fn check_restart_flag() -> Result<bool, String> {
    let flag_path = crate::cache_dir().join(".restart_pending");
    Ok(flag_path.exists())
}

/// Apply a downloaded self-update (.loader.exe.new) and restart.
/// Saves state, spawns a helper script to replace the running exe,
/// then exits. The helper script handles the atomic swap + relaunch.
#[tauri::command]
pub async fn apply_update_and_restart(
    state: State<'_, AppState>,
) -> Result<(), String> {
    let cache = crate::cache_dir();
    let new_exe = cache.join(".loader.exe.new");
    let current_exe = std::env::current_exe()
        .map_err(|e| format!("current_exe: {e}"))?;

    if !new_exe.exists() {
        return Err("No pending update found (.loader.exe.new missing)".to_string());
    }

    tracing::info!(
        "self-update: replacing {} with {}",
        current_exe.display(),
        new_exe.display()
    );

    // Save state before restart (same as prepare_restart).
    prepare_restart_inner(state).await?;

    // Determine the final exe name (current_exe might be loader.exe or
    // a renamed copy; always use "loader.exe" for the replacement).
    let target_dir = current_exe.parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let target_exe = target_dir.join("loader.exe");

    #[cfg(windows)]
    {
        let script_path = cache.join("_update.bat");
        let script = format!(
            "@echo off\r\n\
             :wait\r\n\
             timeout /t 1 /nobreak >nul\r\n\
             move /Y \"{}\" \"{}\"\r\n\
             start \"\" \"{}\"\r\n\
             del \"%~f0\"\r\n",
            new_exe.display(),
            target_exe.display(),
            target_exe.display(),
        );
        std::fs::write(&script_path, &script)
            .map_err(|e| format!("write update script: {e}"))?;

        tracing::info!("self-update: spawning update script {}", script_path.display());
        std::process::Command::new("cmd")
            .args(["/C", &script_path.to_string_lossy()])
            .current_dir(&target_dir)
            .spawn()
            .map_err(|e| format!("spawn update script: {e}"))?;
    }

    #[cfg(not(windows))]
    {
        let script_path = cache.join("_update.sh");
        let script = format!(
            "#!/bin/sh\n\
             sleep 1\n\
             mv \"{}\" \"{}\"\n\
             chmod +x \"{}\"\n\
             \"{}\" &\n\
             rm \"$0\"\n",
            new_exe.display(),
            target_exe.display(),
            target_exe.display(),
            target_exe.display(),
        );
        std::fs::write(&script_path, &script)
            .map_err(|e| format!("write update script: {e}"))?;

        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
            .ok();

        tracing::info!("self-update: spawning update script {}", script_path.display());
        std::process::Command::new("sh")
            .arg(&script_path)
            .current_dir(&target_dir)
            .spawn()
            .map_err(|e| format!("spawn update script: {e}"))?;
    }

    tracing::info!("self-update: exiting for restart...");
    std::process::exit(0);
}
