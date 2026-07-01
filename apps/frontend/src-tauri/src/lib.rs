//! Remote AI IDE — Tauri v2 Desktop Backend.

pub mod commands;
pub mod connection;
pub mod transport;
pub mod bootstrap;
pub mod manifest;
pub mod store;
pub mod updater;

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

/// Log to BOTH stderr AND the log file.  On Windows GUI stderr goes to
/// /dev/null, so the file is the only place you'll see the message.
/// Usage: `log!(&log_path, "msg {}", arg);`
macro_rules! log_msg {
    ($path:expr, $($arg:tt)*) => {{
        use std::io::Write;
        let msg = format!($($arg)*);
        let _ = std::io::stderr().write_all(msg.as_bytes());
        let _ = std::io::stderr().write_all(b"\n");
        let _ = std::io::stderr().flush();
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open($path)
        {
            let _ = writeln!(f, "{msg}");
            let _ = f.flush();
        }
    }};
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Platform-specific cache directory used by both the OTA loader
/// (to download assets) and the main app (to load them at runtime).
pub fn cache_dir() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(dir) = std::env::var("LOCALAPPDATA") {
            return std::path::PathBuf::from(dir).join("remote-ai-ide").join("cache");
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
            return std::path::PathBuf::from(dir).join("remote-ai-ide").join("cache");
        }
        if let Ok(dir) = std::env::var("HOME") {
            return std::path::PathBuf::from(dir).join(".local").join("share").join("remote-ai-ide").join("cache");
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(dir) = std::env::var("HOME") {
            return std::path::PathBuf::from(dir).join("Library").join("Application Support").join("remote-ai-ide").join("cache");
        }
    }
    std::path::PathBuf::from("./cache")
}

/// Guess MIME type from file extension for static-file serving.
fn mime_guess_for(path: &std::path::Path) -> String {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") | Some("htm") => "text/html",
        Some("js") => "application/javascript",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("wasm") => "application/wasm",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
    .to_string()
}

pub fn run() {
    let log_path = std::env::temp_dir().join("remote-ai-ide.log");
    let log_path_str = log_path.to_string_lossy().to_string();

    // ── Panic hook: log panics before the process exits ──
    let log_path_panic = log_path.clone();
    std::panic::set_hook(Box::new(move |info| {
        use std::io::Write;
        let msg = format!(
            "[remote-ai-ide] PANIC: {} | location: {:?}",
            info,
            info.location()
        );
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path_panic)
        {
            let _ = writeln!(f, "{msg}");
            let _ = f.flush();
        }
        let _ = std::io::stderr().write_all(msg.as_bytes());
        let _ = std::io::stderr().write_all(b"\n");
    }));

    let log_file = std::fs::File::create(&log_path)
        .expect("Failed to create log file");
    log_msg!(&log_path_str, "[remote-ai-ide] Log file: {}", log_path_str);

    // ── Print embedded version (from manifest embedded at compile time) ──
    let embedded_version: String =
        serde_json::from_str::<serde_json::Value>(manifest::EMBEDDED_MANIFEST_JSON)
            .ok()
            .and_then(|v| v.get("version").and_then(|ver| ver.as_str()).map(String::from))
            .unwrap_or_else(|| "unknown".to_string());
    log_msg!(&log_path_str, "[remote-ai-ide] ========================================");
    log_msg!(&log_path_str, "[remote-ai-ide]  Version: {embedded_version}");
    log_msg!(&log_path_str, "[remote-ai-ide] ========================================");

    // Set up tracing subscriber (writes to stderr + file via DebugWriter)
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

    log_msg!(&log_path_str, "[remote-ai-ide] Backend starting (pid={})", std::process::id());

    // Log env vars for debug
    match std::env::var("REMOTE_AI_IDE_DEV_URL") {
        Ok(url) => log_msg!(&log_path_str, "[remote-ai-ide] REMOTE_AI_IDE_DEV_URL={url}"),
        Err(_) => log_msg!(&log_path_str, "[remote-ai-ide] REMOTE_AI_IDE_DEV_URL not set"),
    }

    log_msg!(&log_path_str, "[remote-ai-ide] Building Tauri app...");
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init());

    // ── Register custom protocol: cache:// → local cache directory ──
    // The OTA loader downloads updated frontend/ to the cache directory.
    // This protocol serves those files so we don't need to embed them.
    let cache_for_protocol = cache_dir();
    log_msg!(&log_path_str, "[remote-ai-ide] Cache dir: {}", cache_for_protocol.display());
    let frontend_cache = cache_for_protocol.join("frontend");

    let builder = builder.register_uri_scheme_protocol("cache", move |_ctx, request| {
        let uri = request.uri();
        // URI path on cache:// is like "/index.html" or "/assets/index-xxx.js"
        let path = uri.path().trim_start_matches('/');
        let resolved = frontend_cache.join(path);

        // Prevent directory traversal.
        if !resolved.starts_with(&frontend_cache) {
            return tauri::http::Response::builder()
                .status(403)
                .body(Vec::new())
                .unwrap();
        }

        match std::fs::read(&resolved) {
            Ok(data) => {
                let mime = mime_guess_for(&resolved);
                tauri::http::Response::builder()
                    .status(200)
                    .header("Content-Type", mime)
                    .body(data)
                    .unwrap()
            }
            Err(_) => {
                tauri::http::Response::builder()
                    .status(404)
                    .body(Vec::new())
                    .unwrap()
            }
        }
    });

    log_msg!(&log_path_str, "[remote-ai-ide] Tauri builder created, adding setup + invoke_handler...");

    let log_path_clone = log_path_str.clone();
    let cache_for_window = cache_dir();
    builder
        .setup(move |app| {
            let app_handle = app.handle().clone();
            log_msg!(&log_path_clone, "[remote-ai-ide] >>> setup closure entered <<<");

            // ── Compare embedded manifest vs cached manifest ──────────
            // If embedded is newer (user downloaded a new loader.exe), clear
            // the cache so embedded assets win over stale OTA-cached ones.
            // Otherwise keep the cache (OTA updates or same version).
            let embedded_manifest: manifest::Manifest =
                serde_json::from_str(manifest::EMBEDDED_MANIFEST_JSON)
                    .unwrap_or_else(|e| {
                        log_msg!(&log_path_clone, "[remote-ai-ide] Failed to parse embedded manifest: {e}");
                        manifest::Manifest {
                            version: "0.0.0.dev".into(),
                            files: std::collections::HashMap::new(),
                        }
                    });
            log_msg!(&log_path_clone, "[remote-ai-ide] Embedded manifest version: {} ({} files)",
                embedded_manifest.version, embedded_manifest.files.len());

            let cache_manifest_path = cache_for_window.join("manifest.json");
            if cache_manifest_path.exists() {
                if let Ok(cache_json) = std::fs::read_to_string(&cache_manifest_path) {
                    if let Ok(cache_manifest) = serde_json::from_str::<manifest::Manifest>(&cache_json) {
                        log_msg!(&log_path_clone, "[remote-ai-ide] Cache manifest version: {} ({} files)",
                            cache_manifest.version, cache_manifest.files.len());

                        if embedded_manifest.version > cache_manifest.version {
                            log_msg!(&log_path_clone,
                                "[remote-ai-ide] Embedded ({}) newer than cache ({}) — clearing cache",
                                embedded_manifest.version, cache_manifest.version);

                            // Remove all files and dirs in the cache dir.
                            if let Ok(entries) = std::fs::read_dir(&cache_for_window) {
                                for entry in entries.flatten() {
                                    let path = entry.path();
                                    if path.is_dir() {
                                        let _ = std::fs::remove_dir_all(&path);
                                        log_msg!(&log_path_clone, "[remote-ai-ide] Removed cache dir: {}", path.display());
                                    } else {
                                        let _ = std::fs::remove_file(&path);
                                    }
                                }
                            }

                            // Write embedded manifest as the new cache baseline
                            // so OTA checks have correct SHA hashes to compare against.
                            if let Err(e) = std::fs::write(
                                &cache_manifest_path,
                                manifest::EMBEDDED_MANIFEST_JSON,
                            ) {
                                log_msg!(&log_path_clone, "[remote-ai-ide] Warning: failed to write embedded manifest to cache: {e}");
                            } else {
                                log_msg!(&log_path_clone, "[remote-ai-ide] Embedded manifest written to cache as baseline");
                            }
                        } else {
                            log_msg!(&log_path_clone,
                                "[remote-ai-ide] Cache ({}) >= embedded ({}) — using cache",
                                cache_manifest.version, embedded_manifest.version);
                        }
                    }
                }
            } else {
                log_msg!(&log_path_clone, "[remote-ai-ide] No cache manifest — will use embedded assets");
            }

            // ── Create main window: prefer cache > dev server > embedded ──
            //   1. REMOTE_AI_IDE_DEV_URL  →  remote Vite dev server
            //   2. cache/frontend/index.html exists  →  cache:// protocol
            //   3. embedded (built-in fallback)
            use tauri::WebviewUrl;
            use tauri::WebviewWindowBuilder;

            let cache_frontend_index = cache_for_window.join("frontend").join("index.html");

            let window_result = if let Ok(dev_url) = std::env::var("REMOTE_AI_IDE_DEV_URL") {
                log_msg!(&log_path_clone, "[remote-ai-ide] DEV MODE: loading frontend from {dev_url}");
                if let Ok(u) = dev_url.parse::<tauri::Url>() {
                    WebviewWindowBuilder::new(app, "main", WebviewUrl::External(u))
                        .title("Remote AI IDE [DEV]")
                        .inner_size(1600.0, 1000.0)
                        .min_inner_size(900.0, 600.0)
                        .resizable(true)
                        .maximized(true)
                        .visible(true)
                        .build()
                        .map_err(|e| format!("{e}"))
                } else {
                    Err(format!("failed to parse dev URL: {dev_url}"))
                }
            } else if cache_frontend_index.exists() {
                log_msg!(&log_path_clone, "[remote-ai-ide] Loading frontend from cache: {}", cache_frontend_index.display());
                let cache_url = "cache://localhost/index.html"
                    .parse::<tauri::Url>()
                    .map_err(|e| format!("parse cache URL: {e}"))?;
                WebviewWindowBuilder::new(app, "main", WebviewUrl::CustomProtocol(cache_url))
                    .title("Remote AI IDE")
                    .inner_size(1600.0, 1000.0)
                    .min_inner_size(900.0, 600.0)
                    .resizable(true)
                    .maximized(true)
                    .visible(true)
                    .build()
                    .map_err(|e| format!("{e}"))
            } else {
                log_msg!(&log_path_clone, "[remote-ai-ide] Using embedded frontend (cache not found)");
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .title("Remote AI IDE")
                    .inner_size(1600.0, 1000.0)
                    .min_inner_size(900.0, 600.0)
                    .resizable(true)
                    .maximized(true)
                    .visible(true)
                    .build()
                    .map_err(|e| format!("{e}"))
            };

            match &window_result {
                Ok(_) => log_msg!(&log_path_clone, "[remote-ai-ide] Main window created successfully"),
                Err(e) => log_msg!(&log_path_clone, "[remote-ai-ide] FAILED to create main window: {e}"),
            }

            log_msg!(&log_path_clone, "[remote-ai-ide] Step 1: checking agent transport...");
            let agent_transport: Option<Arc<dyn transport::Transport>> =
                if cfg!(target_os = "linux") {
                    match transport::ipc::IpcTransport::spawn() {
                        Ok(t) => {
                            log_msg!(&log_path_clone, "[remote-ai-ide] Local agent started via IPC OK");
                            Some(Arc::new(t))
                        }
                        Err(e) => {
                            log_msg!(&log_path_clone, "[remote-ai-ide] Local agent not available: {e}");
                            None
                        }
                    }
                } else {
                    log_msg!(&log_path_clone, "[remote-ai-ide] Non-Linux host — agent runs on remote machine");
                    None
                };

            log_msg!(&log_path_clone, "[remote-ai-ide] Step 2: creating connection manager...");
            let connections = connection::manager::ConnectionManager::new(app_handle.clone());
            log_msg!(&log_path_clone, "[remote-ai-ide] Step 2 done");

            log_msg!(&log_path_clone, "[remote-ai-ide] Step 3: opening database...");
            let db = store::Database::open().expect("Failed to open database");
            log_msg!(&log_path_clone, "[remote-ai-ide] Step 3 done");

            // Restore persisted connections from DB into memory
            log_msg!(&log_path_clone, "[remote-ai-ide] Step 3.5: loading persisted connections...");
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
                    log_msg!(&log_path_clone, "[remote-ai-ide] Loaded {} persisted connection(s)", records.len());
                }
                Err(e) => {
                    log_msg!(&log_path_clone, "[remote-ai-ide] Warning: failed to load persisted connections: {e}");
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
            log_msg!(&log_path_clone, "[remote-ai-ide] Step 4: state managed");

            // For the local IPC agent (Linux), run the same per-connection demux
            // relay so its recv() stream is owned and acks/output are fanned out.
            // SSH connections spawn their own relay in the connect command.
            if let Some(t) = agent_transport_for_relay {
                let handle = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    commands::session::connection_demux_relay(t, handle, "local".to_string()).await;
                });
                log_msg!(&log_path_clone, "[remote-ai-ide] Step 5: local agent demux relay spawned");
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

            // ── Restart flag: was this a graceful restart for an update? ──
            let restart_flag = cache_for_window.join(".restart_pending");
            let is_graceful_restart = restart_flag.exists();
            if is_graceful_restart {
                log_msg!(&log_path_clone, "[remote-ai-ide] Graceful restart detected — flag at {}", restart_flag.display());
                // Restore connections that were active before the restart.
                // Connection configs (host, port, user, auth) are already in SQLite,
                // loaded above by load_connections().  The flag just tells us which
                // ones were actually connected so we can try to re-establish them.
                if let Ok(flag_data) = std::fs::read_to_string(&restart_flag) {
                    if let Ok(info) = serde_json::from_str::<serde_json::Value>(&flag_data) {
                        if let Some(conns) = info.get("active_connections").and_then(|c| c.as_array()) {
                            log_msg!(&log_path_clone, "[remote-ai-ide] Restart: {} connection(s) were active before restart", conns.len());
                            for conn in conns {
                                let label = conn.get("label").and_then(|v| v.as_str()).unwrap_or("");
                                let host = conn.get("host").and_then(|v| v.as_str()).unwrap_or("");
                                let port: u16 = conn.get("port").and_then(|v| v.as_u64()).unwrap_or(22) as u16;
                                let user = conn.get("user").and_then(|v| v.as_str()).unwrap_or("");
                                log_msg!(&log_path_clone, "[remote-ai-ide] Restart: connection '{label}' ({host}:{port}) was active — ready to reconnect");
                                // Connection record already loaded from SQLite.
                                // The frontend will see it and can reconnect on user action or auto-connect.
                                let app_state = app_handle.state::<AppState>();
                                if app_state.connections.get(label).is_none() {
                                    app_state.connections.create_connection(label.to_string(), label.to_string(), host.to_string(), port, user.to_string());
                                }
                            }
                        }
                        if let Some(sessions) = info.get("active_sessions").and_then(|s| s.as_array()) {
                            log_msg!(&log_path_clone, "[remote-ai-ide] Restart: {} session(s) were active before restart", sessions.len());
                            for sess in sessions {
                                let sid = sess.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
                                let cid = sess.get("connection_id").and_then(|v| v.as_str()).unwrap_or("");
                                log_msg!(&log_path_clone, "[remote-ai-ide] Restart: session {sid} was on connection {cid}");
                                // Re-link session to connection so UI can recover.
                                app_handle.state::<AppState>().session_connections.insert(sid.to_string(), cid.to_string());
                            }
                        }
                    }
                }
                // Remove the flag so we don't re-trigger on next normal start.
                std::fs::remove_file(&restart_flag).ok();
                log_msg!(&log_path_clone, "[remote-ai-ide] Restart flag removed — connections restored, user can reconnect");
            }

            // ── Background OTA updater ──
            log_msg!(&log_path_clone, "[remote-ai-ide] Spawning background updater...");
            let updater_cache = cache_for_window.clone();
            let updater_handle = app_handle.clone();
            updater::spawn_background_updater(updater_handle, updater_cache);
            log_msg!(&log_path_clone, "[remote-ai-ide] Background updater spawned");

            tracing::info!("Remote AI IDE backend initialized");
            log_msg!(&log_path_clone, "[remote-ai-ide] Setup complete OK");
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
            commands::files::git_branches,
            commands::files::git_checkout,
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
            commands::llm::copilot_device_start,
            commands::llm::copilot_device_poll,
            commands::llm::llm_fetch_models,
            commands::llm::load_llm_providers,
            commands::llm::save_llm_providers,
            commands::llm::load_active_model,
            commands::llm::save_active_model,
            commands::tap::load_tap_settings,
            commands::tap::save_tap_settings,
            commands::tap::read_tap_traces,
            commands::tap::clear_tap_traces,
            commands::tap::load_tap_exchanges_db,
            commands::tap::clear_tap_exchanges_db,
            commands::approval::respond_approval,
            commands::config::load_app_config,
            commands::config::save_app_config,
            commands::restart::prepare_restart,
            commands::restart::check_restart_flag,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            log_msg!(&log_path_str, "[remote-ai-ide] FATAL: Tauri run() failed: {e}");
            panic!("Tauri run failed: {e}");
        });
    log_msg!(&log_path_str, "[remote-ai-ide] run() returned — process exiting normally");
}
