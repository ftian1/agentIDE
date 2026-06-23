//! Tauri IPC commands for agent backend settings.
//!
//! Persists the "Agent Backend Settings" modal (Claude / Aider / MCP) into the
//! SQLite `settings` KV table as a JSON blob under the `agent_settings` key.

use serde_json::Value;
use tauri::State;

use crate::AppState;

const KEY: &str = "agent_settings";

/// Load persisted agent settings. Returns null if none have been saved yet.
#[tauri::command]
pub async fn load_agent_settings(state: State<'_, AppState>) -> Result<Option<Value>, String> {
    match state.db.get_setting(KEY)? {
        Some(json) => {
            let value: Value =
                serde_json::from_str(&json).map_err(|e| format!("parse settings: {e}"))?;
            Ok(Some(value))
        }
        None => Ok(None),
    }
}

/// Persist agent settings as a JSON blob.
#[tauri::command]
pub async fn save_agent_settings(
    state: State<'_, AppState>,
    settings: Value,
) -> Result<(), String> {
    let json = serde_json::to_string(&settings).map_err(|e| format!("encode settings: {e}"))?;
    state.db.set_setting(KEY, &json)?;
    Ok(())
}
