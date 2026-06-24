//! Bridge Rust backend tracing → frontend AgentStdout panel.
//!
//! Calls `emit_backend_log()` at key decision points (spawn, write_input,
//! transport resolution, connection events) so the user can see what the
//! backend is doing without tailing the log file.

use tauri::Emitter;

/// Emit a backend log line to the frontend `backend:log` event.
/// The frontend `initAgentLogListeners` pushes it into `useAgentLogStore`.
pub fn emit_backend_log(app_handle: &tauri::AppHandle, msg: &str) {
    let _ = app_handle.emit("backend:log", serde_json::json!({
        "text": msg,
        "ts": chrono::Utc::now().timestamp_millis(),
    }));
}

/// Convenience macro for formatted log emission.
#[macro_export]
macro_rules! backend_log {
    ($app:expr, $($arg:tt)*) => {
        $crate::commands::log_bridge::emit_backend_log($app, &format!($($arg)*))
    };
}
