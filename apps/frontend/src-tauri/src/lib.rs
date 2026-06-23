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
    /// Active transport to the local agent (IPC, Linux only).
    pub agent_transport: tokio::sync::RwLock<Option<Arc<dyn transport::Transport>>>,
    /// Per-connection SSH transports: connection_id → transport.
    pub connection_transports: dashmap::DashMap<String, Arc<dyn transport::Transport>>,
    /// Session → connection mapping: session_id → connection_id.
    /// Used by write_input / resize_terminal / close_session to find the
    /// correct transport for a given session.
    pub session_connections: dashmap::DashMap<String, String>,
    /// Per-connection SSH sessions: connection_id → session.
    /// Used by list_files to run shell commands on the remote host.
    pub ssh_sessions: dashmap::DashMap<String, std::sync::Arc<connection::ssh::SshSession>>,
    /// Pending spawn-ack waiters: session_id → oneshot sender.
    /// A single per-connection demux relay owns each transport's recv() and
    /// fires the matching waiter when a SpawnSessionAck/Nack arrives — so
    /// spawn_session never competes with the relay for the recv() stream.
    /// Ok((pid, tool_version)) on ack, Err(reason) on nack.
    pub pending_acks: dashmap::DashMap<String, tokio::sync::oneshot::Sender<Result<(u32, Option<String>), String>>>,
}

#[derive(Debug, Clone)]
pub struct AgentMessage {
    pub connection_id: String,
    pub message: ProtocolMessage,
}

/// Writes tracing output to both stderr and a file.
/// On Windows GUI, stderr is discarded — use `tail -f %TEMP%/remote-ai-ide.log`
/// or DebugView via the `terminal:debug-log` event.
struct DebugWriter {
    file: std::sync::Mutex<std::fs::File>,
}
impl std::io::Write for DebugWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(buf);
        self.file.lock().unwrap().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let _ = std::io::stderr().flush();
        self.file.lock().unwrap().flush()
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let log_path = std::env::temp_dir().join("remote-ai-ide.log");
    let log_file = std::fs::File::create(&log_path)
        .expect("Failed to create log file");
    // Write to BOTH stderr (console) and file
    let writer = DebugWriter { file: std::sync::Mutex::new(log_file) };
    tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(writer))
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
            eprintln!("[remote-ai-ide] Step 1: checking agent transport...");
            let agent_transport: Option<Arc<dyn transport::Transport>> =
                if cfg!(target_os = "linux") {
                    match transport::ipc::IpcTransport::spawn() {
                        Ok(t) => {
                            eprintln!("[remote-ai-ide] Local agent started via IPC ✓");
                            Some(Arc::new(t))
                        }
                        Err(e) => {
                            eprintln!("[remote-ai-ide] Local agent not available: {e}");
                            None
                        }
                    }
                } else {
                    eprintln!("[remote-ai-ide] Non-Linux host — agent runs on remote machine");
                    None
                };

            eprintln!("[remote-ai-ide] Step 2: creating connection manager...");
            let connections = connection::manager::ConnectionManager::new(app_handle.clone());
            eprintln!("[remote-ai-ide] Step 2 done");

            eprintln!("[remote-ai-ide] Step 3: opening database...");
            let db = store::Database::open().expect("Failed to open database");
            eprintln!("[remote-ai-ide] Step 3 done");

            // Restore persisted connections from DB into memory
            eprintln!("[remote-ai-ide] Step 3.5: loading persisted connections...");
            match db.load_connections() {
                Ok(records) => {
                    for rec in &records {
                        connections.create_connection(
                            rec.id.clone(),
                            rec.label.clone(),
                            rec.host.clone(),
                            rec.port,
                            rec.user.clone(),
                        );
                    }
                    eprintln!("[remote-ai-ide] Loaded {} persisted connection(s)", records.len());
                }
                Err(e) => {
                    eprintln!("[remote-ai-ide] Warning: failed to load persisted connections: {e}");
                }
            }

            let agent_transport_for_relay = agent_transport.clone();
            let state = AppState {
                connections,
                db,
                agent_transport: tokio::sync::RwLock::new(agent_transport),
                connection_transports: dashmap::DashMap::new(),
                session_connections: dashmap::DashMap::new(),
                ssh_sessions: dashmap::DashMap::new(),
                pending_acks: dashmap::DashMap::new(),
            };

            app.manage(state);
            eprintln!("[remote-ai-ide] Step 4: state managed");

            // For the local IPC agent (Linux), run the same per-connection demux
            // relay so its recv() stream is owned and acks/output are fanned out.
            // SSH connections spawn their own relay in the connect command.
            if let Some(t) = agent_transport_for_relay {
                let handle = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    commands::session::connection_demux_relay(t, handle, "local".to_string()).await;
                });
                eprintln!("[remote-ai-ide] Step 5: local agent demux relay spawned");
            }

            // Log WebView lifecycle for debugging renderer crashes
            if let Some(window) = app_handle.get_webview_window("main") {
                window.on_window_event(|event| {
                    match event {
                        tauri::WindowEvent::CloseRequested { .. } => {
                            eprintln!("[remote-ai-ide] Window CloseRequested");
                        }
                        tauri::WindowEvent::Destroyed => {
                            eprintln!("[remote-ai-ide] Window Destroyed");
                        }
                        _ => {}
                    }
                });
            }

            tracing::info!("Remote AI IDE backend initialized");
            eprintln!("[remote-ai-ide] Setup complete ✓");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::connection::connect,
            commands::connection::disconnect,
            commands::connection::list_connections,
            commands::connection::list_ssh_configs,
            commands::files::list_files,
            commands::files::read_file,
            commands::files::write_file,
            commands::session::spawn_session,
            commands::session::close_session,
            commands::session::resize_terminal,
            commands::session::write_input,
            commands::session::send_agent_message,
            commands::session::apply_code_change,
            commands::session::reject_code_change,
            commands::tool::probe_tool,
            commands::tool::install_tool,
            commands::settings::load_agent_settings,
            commands::settings::save_agent_settings,
            commands::approval::respond_approval,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
