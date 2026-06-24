//! Tauri IPC commands for the HTTP traffic tap.
//!
//! Persists tap settings (enabled + mode) in the SQLite `settings` KV table,
//! appends each captured exchange to a per-connection JSONL trace file, and
//! reloads persisted traces on demand.

use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::commands::session::HttpTrafficEvent;
use crate::AppState;

const KEY: &str = "tap_settings";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TapSettings {
    #[serde(default)]
    pub enabled: bool,
    /// "mitm" | "reverse"
    #[serde(default = "default_mode")]
    pub mode: String,
}

fn default_mode() -> String {
    "mitm".to_string()
}

impl Default for TapSettings {
    fn default() -> Self {
        // Tap is ON by default so HTTP traffic is captured out of the box.
        Self { enabled: true, mode: default_mode() }
    }
}

/// Read tap (enabled, mode) from the DB.
/// When no setting is persisted, defaults to enabled + mitm.
pub fn tap_control(state: &AppState) -> Option<(bool, String)> {
    match state.db.get_setting(KEY) {
        Ok(Some(raw)) => {
            let s: TapSettings = serde_json::from_str(&raw).ok()?;
            Some((s.enabled, s.mode))
        }
        Ok(None) => {
            // No persisted preference — default ON.
            Some((true, "mitm".to_string()))
        }
        Err(_) => None,
    }
}

/// Directory where JSONL traces are persisted.
fn traces_dir() -> PathBuf {
    let data_dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    data_dir.join("remote-ai-ide").join("traces")
}

fn trace_file(connection_id: &str) -> PathBuf {
    // One file per connection. Sanitize the id for filesystem safety.
    let safe: String = connection_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    traces_dir().join(format!("{safe}.jsonl"))
}

/// Append a captured exchange to the connection's JSONL trace (best-effort).
pub fn append_trace(connection_id: &str, evt: &HttpTrafficEvent) {
    let dir = traces_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let Ok(line) = serde_json::to_string(evt) else { return };
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(trace_file(connection_id))
    {
        let _ = writeln!(f, "{line}");
    }
}

#[tauri::command]
pub async fn load_tap_settings(state: State<'_, AppState>) -> Result<Option<Value>, String> {
    match state.db.get_setting(KEY)? {
        Some(json) => {
            let value: Value =
                serde_json::from_str(&json).map_err(|e| format!("parse tap settings: {e}"))?;
            Ok(Some(value))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn save_tap_settings(state: State<'_, AppState>, settings: Value) -> Result<(), String> {
    let json = serde_json::to_string(&settings).map_err(|e| format!("encode tap settings: {e}"))?;
    state.db.set_setting(KEY, &json)?;
    Ok(())
}

/// Re-read persisted exchanges for a connection (most recent last).
#[tauri::command]
pub async fn read_tap_traces(connection_id: String) -> Result<Vec<Value>, String> {
    let path = trace_file(&connection_id);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(format!("read traces: {e}")),
    };
    let mut out = Vec::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            out.push(v);
        }
    }
    Ok(out)
}

/// Delete the persisted trace file for a connection.
#[tauri::command]
pub async fn clear_tap_traces(connection_id: String) -> Result<(), String> {
    let path = trace_file(&connection_id);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("clear traces: {e}")),
    }
}

/// Save a captured exchange to the SQLite DB (called from demux relay).
pub fn save_exchange_to_db(state: &AppState, rec: &crate::store::TapExchangeRecord) {
    if let Err(e) = state.db.insert_tap_exchange(rec) {
        tracing::warn!(error = %e, "Failed to persist tap exchange to DB");
    }
}

/// Load exchanges from DB (paginated).
#[tauri::command]
pub async fn load_tap_exchanges_db(
    state: State<'_, AppState>,
    connection_id: String,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<crate::store::TapExchangeRecord>, String> {
    state.db.load_tap_exchanges(&connection_id, limit.unwrap_or(500), offset.unwrap_or(0))
        .map_err(|e| format!("load exchanges: {e}"))
}

/// Clear all exchanges for a connection from DB.
#[tauri::command]
pub async fn clear_tap_exchanges_db(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<(), String> {
    state.db.clear_tap_exchanges(&connection_id)
}
