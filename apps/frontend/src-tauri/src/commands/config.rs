//! Generic app-config persistence (layout, agent-engine profiles, etc.).
//!
//! Each frontend store serialises its state to JSON and calls
//!   load_app_config("key") / save_app_config("key", json)
//! which delegates to the SQLite `settings` KV table.

use tauri::State;
use crate::AppState;

#[tauri::command]
pub async fn load_app_config(state: State<'_, AppState>, key: String) -> Result<Option<String>, String> {
    state.db.get_setting(&key).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_app_config(state: State<'_, AppState>, key: String, value: String) -> Result<(), String> {
    state.db.set_setting(&key, &value).map_err(|e| e.to_string())
}
