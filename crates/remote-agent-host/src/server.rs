//! Server — main event loop for the Remote Agent Host.
//!
//! Receives [`ProtocolMessage`] frames from the transport layer,
//! dispatches each message to the appropriate handler, and sends
//! responses back.

use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

use shared_protocol::messages::ProtocolMessage;
use shared_protocol::types::*;

use crate::pty::worker::{self};
use crate::session::manager::SessionManager;
use crate::session::registry::SessionRegistry;
use crate::session::types::{PtyOp, Session};

/// Check if a binary exists on PATH.
fn which(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run a shell command and return (success, stdout, stderr).
fn run_sh(cmd: &str) -> (bool, String, String) {
    match Command::new("bash").arg("-c").arg(cmd).output() {
        Ok(o) => (
            o.status.success(),
            String::from_utf8_lossy(&o.stdout).trim().to_string(),
            String::from_utf8_lossy(&o.stderr).trim().to_string(),
        ),
        Err(e) => (false, String::new(), e.to_string()),
    }
}

/// Try to install Node.js + npm if not already present.
/// Prefers user-local install (no root needed) over system-wide.
fn ensure_nodejs() -> Result<(), String> {
    if which("npm") {
        // Verify npm actually works (not a broken copy from a prior failed install)
        let (ok, ver, _) = run_sh("npm --version 2>&1");
        if ok && !ver.is_empty() {
            tracing::info!(%ver, "npm found and working");
            return Ok(());
        }
        tracing::warn!("npm found but broken (prior partial install?), re-installing...");
    }
    tracing::info!("npm not found (or broken), trying to install Node.js...");

    // Strategy 1 (no root): download Node.js binary to ~/.local/bin
    let mut last_download_err: Option<String> = None;
    if which("curl") || which("wget") {
        let using_curl = which("curl");
        let fetcher = if using_curl { "curl" } else { "wget" };
        tracing::info!(fetcher, "Downloading Node.js to ~/.local/bin (no root)...");
        let install_sh = format!(r#"
            set -e
            NODE_VER="20.18.0"
            ARCH=$(uname -m)
            [ "$ARCH" = "x86_64" ] && ARCH="x64"
            [ "$ARCH" = "aarch64" ] && ARCH="arm64"
            TARBALL="node-v${{NODE_VER}}-linux-${{ARCH}}.tar.xz"
            URL="https://nodejs.org/dist/v${{NODE_VER}}/${{TARBALL}}"
            echo "Downloading $URL" >&2
            cd ~
            if [ "{fetcher}" = "curl" ]; then
                curl -fsSL "$URL" -o "$TARBALL"
            else
                wget -q "$URL" -O "$TARBALL"
            fi
            echo "Extracting to ~/.local..." >&2
            mkdir -p ~/.local/bin ~/.local/lib
            tar -xf "$TARBALL" -C ~/.local --strip-components=1
            rm -f ~/${{TARBALL}}
            export PATH="$HOME/.local/bin:$PATH"
            echo "Node.js $(~/.local/bin/node --version) installed to ~/.local/bin"
        "#);
        let (ok, stdout, stderr) = run_sh(&install_sh);
        // Check ~/.local/bin/npm directly (PATH set in script doesn't persist)
        let npm_in_local = std::env::var("HOME")
            .map(|h| std::path::PathBuf::from(h).join(".local/bin/npm"))
            .map(|p| p.exists())
            .unwrap_or(false);
        if ok && (which("npm") || npm_in_local) {
            if let Ok(home) = std::env::var("HOME") {
                let local_bin = format!("{}/.local/bin", home);
                if let Ok(current) = std::env::var("PATH") {
                    std::env::set_var("PATH", format!("{}:{}", local_bin, current));
                }
            }
            tracing::info!("Node.js installed to ~/.local/bin (no root)");
            return Ok(());
        }
        let curl_err = format!(
            "curl/wget download failed.\n  stdout: {}\n  stderr: {}",
            if stdout.len() > 500 { &stdout[..500] } else { &stdout },
            if stderr.len() > 500 { &stderr[..500] } else { &stderr }
        );
        tracing::warn!(ok, %curl_err, "Node.js download via curl/wget failed, trying package managers...");
        // Keep stderr for the final error message so the user sees WHY it failed
        last_download_err = Some(stderr.clone());
    } else {
        last_download_err = Some("curl and wget not found".into());
    }

    // Strategy 2 (needs root): system package manager
    let mut last_pkg_err = None;
    let pkg_managers = [
        ("apt-get", "apt-get update -qq && apt-get install -y -qq nodejs npm 2>&1"),
        ("yum", "yum install -y nodejs npm 2>&1"),
        ("dnf", "dnf install -y nodejs npm 2>&1"),
        ("apk", "apk add nodejs npm 2>&1"),
        ("pacman", "pacman -S --noconfirm nodejs npm 2>&1"),
    ];
    for (pm, cmd) in &pkg_managers {
        if which(pm) {
            tracing::info!(%pm, "Trying package manager (may need root)...");
            let (ok, stdout, stderr) = run_sh(cmd);
            if ok && which("npm") {
                tracing::info!("Node.js installed via {}", pm);
                return Ok(());
            }
            tracing::warn!(%pm, %stdout, %stderr, "Package manager failed");
            last_pkg_err = Some(format!("{}: {}", pm, stderr));
        }
    }

    let curl_ok = which("curl");
    let wget_ok = which("wget");
    let extra = if let Some(ref e) = last_download_err {
        format!("Last download error: {}", if e.len() > 200 { &e[..200] } else { e })
    } else if let Some(ref e) = last_pkg_err {
        format!("Last package manager error: {}", if e.len() > 200 { &e[..200] } else { e })
    } else {
        "No install strategies available".into()
    };
    let details = format!(
        "curl_available={curl_ok}, wget_available={wget_ok}\n\
         {extra}\n\
         Hint: the agent log at ~/.remote-agent-host/agent.log has the full error output.\n\
         You can also run the install manually on the remote machine:\n\
         curl -fsSL https://nodejs.org/dist/v20.18.0/node-v20.18.0-linux-x64.tar.xz -o ~/n.tar.xz && \\\n\
         tar -xf ~/n.tar.xz -C ~/.local --strip-components=1 && \\\n\
         export PATH=\"$HOME/.local/bin:$PATH\" && which npm && npm install -g @anthropic-ai/claude-code"
    );
    tracing::error!(%details, "All Node.js install strategies failed");
    Err(format!("Cannot install Node.js. {}", details))
}

/// Check if a tool binary exists on PATH, and auto-install if missing.
/// Returns the resolved command name on success.
fn ensure_tool_installed(tool: &ToolKind) -> Result<String, String> {
    let (command, npm_package) = match tool {
        ToolKind::Claude => ("claude", "@anthropic-ai/claude-code"),
        ToolKind::Copilot => ("gh", "@github/copilot-cli"),
        ToolKind::Custom(name) => {
            if which(name) {
                return Ok(name.clone());
            }
            return Err(format!(
                "Custom tool '{}' not found on remote. Please install it manually.", name
            ));
        }
    };

    // 1. Check if already installed
    if which(command) {
        tracing::info!(%command, "Tool found on PATH");
        return Ok(command.to_string());
    }

    tracing::info!(%command, %npm_package, "Tool not found, auto-installing...");

    // 2. Ensure npm is available (install Node.js if needed)
    ensure_nodejs()?;

    // 3. npm install (user-local prefix, no root). Use full path to npm if in ~/.local/bin.
    let npm_bin = if let Ok(home) = std::env::var("HOME") {
        let p = format!("{}/.local/bin/npm", home);
        if std::path::Path::new(&p).exists() { p } else { "npm".to_string() }
    } else { "npm".to_string() };
    let install_cmd = format!(
        "export PATH=\"$HOME/.npm-global/bin:$HOME/.local/bin:$PATH\"; \
         mkdir -p ~/.npm-global && {} config set prefix ~/.npm-global 2>/dev/null; \
         {} install -g {} 2>&1",
        npm_bin, npm_bin, npm_package
    );
    tracing::info!(%install_cmd, "Running npm install (user-local, no root)...");
    let (ok, stdout, stderr) = run_sh(&install_cmd);
    if !ok {
        return Err(format!(
            "Auto-install of {} failed.\nstdout: {}\nstderr: {}",
            tool.display_name(), stdout, stderr
        ));
    }

    // 4. Verify — search user-local paths first, then system paths
    let user_bins = [
        format!("{}/.npm-global/bin", std::env::var("HOME").unwrap_or_default()),
        format!("{}/.local/bin", std::env::var("HOME").unwrap_or_default()),
    ];
    for bin_dir in &user_bins {
        let full = format!("{}/{}", bin_dir, command);
        if std::path::Path::new(&full).exists() {
            // Add to agent's PATH if not already there
            if let Ok(current) = std::env::var("PATH") {
                if !current.contains(bin_dir) {
                    std::env::set_var("PATH", format!("{}:{}", bin_dir, current));
                }
            }
            tracing::info!(%command, path = %full, "Tool found in user-local bin");
            return Ok(full);
        }
    }
    // Check system paths
    for sys_dir in &["/usr/local/bin", "/usr/bin"] {
        let full = format!("{}/{}", sys_dir, command);
        if std::path::Path::new(&full).exists() {
            return Ok(full);
        }
    }
    if which(command) {
        tracing::info!(%command, "Tool installed successfully");
        Ok(command.to_string())
    } else {
        Err(format!(
            "{} was installed but not found on PATH.\n\
             It may be in ~/.npm-global/bin or ~/.local/bin.\n\
             Try: export PATH=\"$HOME/.npm-global/bin:$HOME/.local/bin:$PATH\"",
            tool.display_name()
        ))
    }
}

/// Top-level server handle.
pub struct Server {
    pub sessions: SessionManager,
    pub registry: Arc<SessionRegistry>,
    pub host_id: String,
    /// Active HTTP tap/gateway proxies, keyed by session id. Dropping a handle stops it.
    tap_proxies: HashMap<String, crate::tap::proxy::ProxyHandle>,
}

impl Server {
    pub fn new() -> Self {
        let registry = Arc::new(SessionRegistry::new());
        let sessions = SessionManager::new(registry.clone());
        Self {
            sessions,
            registry,
            host_id: Uuid::new_v4().to_string(),
            tap_proxies: HashMap::new(),
        }
    }

    /// Dispatch an incoming protocol message.
    /// Returns `Some(response)` if the handler produced a direct response.
    pub async fn dispatch(
        &mut self,
        msg: ProtocolMessage,
        transport_tx: &mpsc::UnboundedSender<ProtocolMessage>,
    ) -> Option<ProtocolMessage> {
        match msg {
            ProtocolMessage::Hello { version, capabilities, session_id: _ } => {
                tracing::info!(version, ?capabilities, "Received Hello");
                None
            }
            ProtocolMessage::HelloAck { version, server_version, server_arch } => {
                tracing::info!(version, %server_version, %server_arch, "Received HelloAck");
                None
            }
            ProtocolMessage::SpawnSession { session_id, tool, args, env, cwd, terminal_cols, terminal_rows, container } => {
                self.handle_spawn(session_id, tool, args, env, cwd, container, terminal_cols, terminal_rows, transport_tx).await
            }
            ProtocolMessage::CloseSession { session_id } => {
                self.handle_close(&session_id).await
            }
            ProtocolMessage::TerminalInput { session_id, data } => {
                self.handle_input(&session_id, data).await;
                None
            }
            ProtocolMessage::TerminalResize { session_id, cols, rows } => {
                self.handle_resize(&session_id, cols, rows).await;
                None
            }
            ProtocolMessage::Ack { session_id, seq, bytes_consumed } => {
                self.handle_ack(&session_id, seq, bytes_consumed).await;
                None
            }
            ProtocolMessage::ProbeRequest { tool } => Some(self.handle_probe(&tool).await),
            ProtocolMessage::InstallRequest { tool, .. } => Some(self.handle_install(&tool).await),
            ProtocolMessage::ApprovalResponse { session_id, request_id, decision } => {
                self.handle_approval_response(&session_id, &request_id, &decision).await;
                None
            }
            _ => {
                tracing::warn!(kind = msg.kind(), "Unexpected message from client");
                None
            }
        }
    }

    async fn handle_spawn(
        &mut self,
        session_id: String,
        tool: ToolKind,
        args: Vec<String>,
        env: HashMap<String, String>,
        cwd: Option<String>,
        container: Option<String>,
        cols: u16,
        rows: u16,
        transport_tx: &mpsc::UnboundedSender<ProtocolMessage>,
    ) -> Option<ProtocolMessage> {
        tracing::info!(%session_id, ?tool, ?container, "Handling SpawnSession");

        let mut env = env;

        // ── MITM proxy (tap + providers) ────────────────────────────
        // All CLI traffic is intercepted via HTTP_PROXY/HTTPS_PROXY →
        // CONNECT tunnel → TLS termination.  Provider routing and auth
        // injection happen in forward() based on model-id matching.
        //
        // Capture the ORIGINAL upstream proxy BEFORE the tap overwrites
        // HTTPS_PROXY — needed for forward() to CONNECT-tunnel outbound.
        let original_upstream_proxy = {
            let from_env_map = env.get("HTTPS_PROXY")
                .or_else(|| env.get("https_proxy"))
                .cloned();
            let from_host = std::env::var("HTTPS_PROXY").ok()
                .or_else(|| std::env::var("https_proxy").ok());
            from_env_map.or(from_host)
                .and_then(|v| crate::tap::parse_upstream_proxy(&v))
        };
        let tap_cfg = crate::tap::TapConfig::take_from_env(&mut env);
        let has_gateway = tap_cfg.gateway_token.is_some()
            || tap_cfg.gateway_provider.is_some();
        let has_providers = !tap_cfg.providers.is_empty();
        let need_proxy = tap_cfg.enabled || has_gateway || has_providers;

        tracing::info!(
            has_gateway,
            has_providers,
            providers_len = tap_cfg.providers.len(),
            enabled = tap_cfg.enabled,
            "tap: starting Mitm proxy"
        );

        if need_proxy {
            let gateway_path_prefix = tap_cfg.gateway_provider.as_deref()
                .and_then(|p| match p {
                    "deepseek" => Some("/anthropic".to_string()),
                    _ => None,
                });
            match crate::tap::proxy::start_session_proxy(
                session_id.clone(),
                transport_tx.clone(),
                None,  // upstream_host — not needed for Mitm
                original_upstream_proxy,
                tap_cfg.gateway_provider.clone(),
                tap_cfg.gateway_token.clone(),
                gateway_path_prefix,
                tap_cfg.providers.clone(),
            ) {
                Ok(handle) => {
                    let port = handle.port;
                    for (k, v) in crate::tap::proxy_env(port, &handle.ca_pem_path) {
                        env.insert(k, v);
                    }
                    self.tap_proxies.insert(session_id.clone(), handle);
                    let gw_label = tap_cfg.gateway_provider.as_deref().unwrap_or("none");
                    tracing::info!(%session_id, port, gateway = %gw_label, providers_len = tap_cfg.providers.len(), "proxy: Mitm enabled");
                }
                Err(e) => {
                    tracing::error!(%session_id, error = %e, "tap: failed to start, continuing without");
                }
            }
        }

        // Auto-detect & install the tool if missing on the remote machine
        let original_cmd = match ensure_tool_installed(&tool) {
            Ok(cmd) => cmd,
            Err(e) => {
                tracing::error!(%session_id, error = %e, "Tool not available");
                return Some(ProtocolMessage::SpawnSessionNack {
                    session_id,
                    reason: e,
                });
            }
        };

        let base_args: Vec<String> = match &tool {
            ToolKind::Copilot => {
                let mut a = vec!["copilot".to_string()];
                a.extend(args.clone());
                a
            }
            _ => args.clone(),
        };

        let (command, exec_args): (String, Vec<String>) = if let Some(ref ctr) = container {
            tracing::info!(%session_id, container = %ctr, "Wrapping command with docker exec");
            let mut a = vec!["exec".to_string(), "-it".to_string(), ctr.clone(), original_cmd];
            a.extend(base_args);
            ("docker".to_string(), a)
        } else {
            (original_cmd, base_args)
        };

        // ── Capture launch info BEFORE exec_args / cwd are moved ──
        let launch_cmd_str = format!("{} {}", command, exec_args.join(" "));
        let launch_cwd = cwd.clone().unwrap_or_default();
        let mut launch_data = std::collections::HashMap::new();
        launch_data.insert("command".to_string(), launch_cmd_str.clone());
        launch_data.insert("cwd".to_string(), launch_cwd.clone());
        for (k, v) in &env {
            if k.starts_with("ANTHROPIC_") || k.starts_with("OPENAI_")
                || k == "TERM" || k.starts_with("__tap") || k.starts_with("__gateway")
            {
                launch_data.insert(k.clone(), v.clone());
            }
        }

        // Ensure TMPDIR exists (workaround for /tmp inode exhaustion on some hosts)
        for key in &["TMPDIR", "TMP", "TEMP"] {
            if let Some(dir) = env.get(&key.to_string()) {
                let _ = std::fs::create_dir_all(dir);
            }
        }

        tracing::info!(%session_id, cmd = %command, args = ?exec_args, cwd = ?cwd, cols, rows,
            http_proxy = ?env.get("HTTP_PROXY"),
            has_gateway = tap_cfg.gateway_provider.is_some(),
            providers = tap_cfg.providers.len(),
            "handle_spawn: launching CLI");
        // Full dump at info level so it always appears in agent.log.
        tracing::info!(%session_id,
            launch_cmd = %launch_cmd_str,
            launch_cwd = %launch_cwd,
            full_env = ?env,
            "handle_spawn: CLI launch details");
        let handles = match worker::spawn_cli(&command, &exec_args, &env, cwd.as_deref(), cols, rows) {
            Ok(h) => h,
            Err(e) => {
                tracing::error!(%session_id, error = %e, "PTY spawn failed");
                return Some(ProtocolMessage::SpawnSessionNack {
                    session_id,
                    reason: format!("PTY spawn failed: {}", e),
                });
            }
        };

        let pid = handles.pid;
        let (pty_op_tx, pty_op_rx) = mpsc::unbounded_channel::<PtyOp>();

        let metadata = SessionMetadata {
            cwd, env: env.clone(), terminal_cols: cols, terminal_rows: rows, args,
        };

        let session = Session::new(
            session_id.clone(), tool.clone(), exec_args, pid,
            metadata, cols, rows, pty_op_tx,
        );

        let session_arc = match self.sessions.register_session(session) {
            Ok(s) => s,
            Err(e) => {
                return Some(ProtocolMessage::SpawnSessionNack {
                    session_id,
                    reason: format!("Registration failed: {:?}", e),
                });
            }
        };

        // Send the captured launch info to the frontend for the AgentStdout panel.
        let _ = transport_tx.send(ProtocolMessage::SessionEvent {
            session_id: session_id.clone(),
            event_type: shared_protocol::types::SessionEventType::ShellCommand,
            data: launch_data,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        });

        let transport_tx2 = transport_tx.clone();
        let registry2 = self.registry.clone();
        let (_read_handle, _write_handle) = worker::run_pty_loop(
            session_arc.clone(), handles, transport_tx2, pty_op_rx, registry2,
        );

        Some(ProtocolMessage::SpawnSessionAck {
            session_id,
            pid,
            tool_version: None,
        })
    }

    async fn handle_close(&mut self, session_id: &str) -> Option<ProtocolMessage> {
        // Stop the unified proxy for this session, if any.
        self.tap_proxies.remove(session_id);
        if let Some(session) = self.sessions.get(session_id) {
            let _ = session.pty_op_tx.send(PtyOp::Shutdown);
        }
        match self.sessions.close_session(session_id).await {
            Ok(exit_code) => Some(ProtocolMessage::CloseSessionAck {
                session_id: session_id.to_string(), exit_code,
            }),
            Err(e) => Some(ProtocolMessage::Error {
                code: ErrorCode::SessionNotFound,
                message: format!("{:?}", e),
                session_id: Some(session_id.to_string()),
            }),
        }
    }

    async fn handle_input(&self, session_id: &str, data: Vec<u8>) {
        let preview = String::from_utf8_lossy(&data[..data.len().min(20)]);
        if let Some(session) = self.sessions.get(session_id) {
            tracing::trace!(%session_id, len = data.len(), %preview, "handle_input: writing to PTY");
            let _ = session.pty_op_tx.send(PtyOp::Write(data));
        } else {
            tracing::warn!(%session_id, %preview, "handle_input: session NOT FOUND");
        }
    }

    /// Route an approval decision back to the agent CLI as a control reply on
    /// its stdin. Different agent CLIs expect different reply tokens; we write a
    /// newline-terminated decision token that the agent's permission prompt reads.
    async fn handle_approval_response(
        &self,
        session_id: &str,
        request_id: &str,
        decision: &ApprovalDecision,
    ) {
        let token = match decision {
            ApprovalDecision::Allow => "allow",
            ApprovalDecision::AllowAll => "allow-all",
            ApprovalDecision::Reject => "reject",
        };
        tracing::info!(%session_id, %request_id, token, "Routing approval response to agent stdin");
        if let Some(session) = self.sessions.get(session_id) {
            let reply = format!("{}\n", token);
            let _ = session.pty_op_tx.send(PtyOp::Write(reply.into_bytes()));
        }
    }

    async fn handle_resize(&self, session_id: &str, cols: u16, rows: u16) {
        if let Some(session) = self.sessions.get(session_id) {
            let _ = session.pty_op_tx.send(PtyOp::Resize { cols, rows });
        }
    }

    async fn handle_ack(&self, session_id: &str, _seq: u64, _bytes_consumed: u64) {
        // Flow control ack is tracked in the session but no explicit PtyOp needed
        if let Some(_session) = self.sessions.get(session_id) {
            // TODO: update session flow control counters
        }
    }

    async fn handle_probe(&self, tool: &ToolKind) -> ProtocolMessage {
        let command = tool.default_command();
        let installed = which(command);
        ProtocolMessage::ProbeResponse {
            tool: tool.clone(),
            installed,
            version: if installed {
                let (_, v, _) = run_sh(&format!("{} --version 2>&1", command));
                Some(v.lines().next().unwrap_or("unknown").to_string())
            } else { None },
            path: if installed {
                let (_, p, _) = run_sh(&format!("which {}", command));
                Some(p)
            } else { None },
            auth_ok: None,
            details: None,
        }
    }

    async fn handle_install(&self, tool: &ToolKind) -> ProtocolMessage {
        match ensure_tool_installed(tool) {
            Ok(_) => ProtocolMessage::InstallComplete {
                tool: tool.clone(),
                success: true,
                version: None,
                error: None,
            },
            Err(e) => ProtocolMessage::InstallComplete {
                tool: tool.clone(),
                success: false,
                version: None,
                error: Some(e),
            },
        }
    }

    pub fn shutdown(&mut self) {
        tracing::info!("Server shutdown initiated — killing all child processes");
        self.registry.shutdown_all();
        tracing::info!("Server shutdown complete");
    }
}
