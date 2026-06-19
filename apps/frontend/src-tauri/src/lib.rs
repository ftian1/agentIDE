//! Remote AI IDE — Tauri v2 Desktop Backend.

pub mod commands;
pub mod connection;
pub mod transport;
pub mod bootstrap;
pub mod store;

use std::sync::Arc;
use tauri::Manager;

use shared_protocol::ProtocolMessage;

/// Application state shared across all Tauri command handlers.
pub struct AppState {
    pub connections: connection::manager::ConnectionManager,
    pub db: store::Database,
    /// Active transport to the (local or remote) agent.
    pub agent_transport: tokio::sync::RwLock<Option<Arc<dyn transport::Transport>>>,
}

#[derive(Debug, Clone)]
pub struct AgentMessage {
    pub connection_id: String,
    pub message: ProtocolMessage,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let log_path = std::env::temp_dir().join("remote-ai-ide.log");
    let log_file = std::fs::File::create(&log_path)
        .expect("Failed to create log file");
    tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(log_file))
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .with_ansi(false)
        .init();

    // Also write a marker so we know the code runs
    eprintln!("[remote-ai-ide] Backend starting...");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(move |app| {
            let app_handle = app.handle().clone();

            eprintln!("[remote-ai-ide] Setup running...");

            // Try IPC only on Linux (agent binary is always Linux).
            // On Windows/macOS, the agent runs on the remote host.
            let agent_transport: Option<Arc<dyn transport::Transport>> =
                if cfg!(target_os = "linux") {
                    match transport::ipc::IpcTransport::spawn() {
                        Ok(t) => {
                            tracing::info!("Local agent started via IPC ✓");
                            Some(Arc::new(t))
                        }
                        Err(e) => {
                            tracing::warn!("Local agent not available: {e}");
                            None
                        }
                    }
                } else {
                    tracing::info!("Non-Linux host — agent runs on remote machine");
                    None
                };

            let state = AppState {
                connections: connection::manager::ConnectionManager::new(app_handle.clone()),
                db: store::Database::open().expect("Failed to open database"),
                agent_transport: tokio::sync::RwLock::new(agent_transport),
            };

            app.manage(state);

            // Spawn agent message relay: polls the transport and emits Tauri events
            let handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                relay_agent_messages(handle).await;
            });

            tracing::info!("Remote AI IDE backend initialized");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::connection::connect,
            commands::connection::disconnect,
            commands::connection::list_connections,
            commands::connection::list_ssh_configs,
            commands::session::spawn_session,
            commands::session::close_session,
            commands::session::resize_terminal,
            commands::session::write_input,
            commands::tool::probe_tool,
            commands::tool::install_tool,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Background task that polls the agent transport and emits Tauri events.
async fn relay_agent_messages(_app_handle: tauri::AppHandle) {
    // We need to access the transport. Since it's in AppState and we can't
    // easily get State from a spawned task, we use a polling approach
    // with Tauri's state API via commands. For now, the session commands
    // will handle message relay directly.
    //
    // This task exists as a placeholder for the full agent message relay
    // that will be implemented when we add persistent transport polling.
    tracing::debug!("Agent message relay task started");
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}
