//! Server — main event loop for the Remote Agent Host.
//!
//! Receives [`ProtocolMessage`] frames from the transport layer,
//! dispatches each message to the appropriate handler, and sends
//! responses back.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

use shared_protocol::messages::ProtocolMessage;
use shared_protocol::types::*;

use crate::pty::worker::{self};
use crate::session::manager::SessionManager;
use crate::session::registry::SessionRegistry;
use crate::session::types::{PtyOp, Session};

/// Top-level server handle.
pub struct Server {
    pub sessions: SessionManager,
    pub registry: Arc<SessionRegistry>,
    pub host_id: String,
}

impl Server {
    pub fn new() -> Self {
        let registry = Arc::new(SessionRegistry::new());
        let sessions = SessionManager::new(registry.clone());
        Self {
            sessions,
            registry,
            host_id: Uuid::new_v4().to_string(),
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
            ProtocolMessage::SpawnSession { session_id, tool, args, env, cwd, terminal_cols, terminal_rows } => {
                self.handle_spawn(session_id, tool, args, env, cwd, terminal_cols, terminal_rows, transport_tx).await
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
            ProtocolMessage::ProbeRequest { tool } => {
                Some(self.handle_probe(&tool).await)
            }
            ProtocolMessage::InstallRequest { tool, version } => {
                self.handle_install(&tool, version, transport_tx).await
            }
            ProtocolMessage::Ping { nonce } => {
                Some(ProtocolMessage::Pong { nonce })
            }
            ProtocolMessage::Pong { .. } => None,
            other => {
                tracing::warn!(kind = other.kind(), "Unexpected message from client");
                Some(ProtocolMessage::Error {
                    code: ErrorCode::InvalidMessage,
                    message: format!("Unexpected message: {}", other.kind()),
                    session_id: other.session_id().map(String::from),
                })
            }
        }
    }

    // ── Handlers ───────────────────────────────────────────

    async fn handle_spawn(
        &mut self,
        session_id: String,
        tool: ToolKind,
        args: Vec<String>,
        env: HashMap<String, String>,
        cwd: Option<String>,
        cols: u16,
        rows: u16,
        transport_tx: &mpsc::UnboundedSender<ProtocolMessage>,
    ) -> Option<ProtocolMessage> {
        tracing::info!(%session_id, ?tool, "Handling SpawnSession");

        let command = match &tool {
            ToolKind::Claude => "claude".to_string(),
            ToolKind::Copilot => "gh".to_string(),
            ToolKind::Custom(cmd) => cmd.clone(),
        };

        let exec_args: Vec<String> = match &tool {
            ToolKind::Copilot => {
                let mut a = vec!["copilot".to_string()];
                a.extend(args.clone());
                a
            }
            _ => args.clone(),
        };

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
        if let Some(session) = self.sessions.get(session_id) {
            let _ = session.pty_op_tx.send(PtyOp::Shutdown);
        }
        match self.sessions.close_session(session_id).await {
            Ok(exit_code) => Some(ProtocolMessage::CloseSessionAck {
                session_id: session_id.into(), exit_code,
            }),
            Err(e) => {
                let msg = format!("Close failed: {:?}", e);
                Some(ProtocolMessage::Error {
                    code: e,
                    message: msg,
                    session_id: Some(session_id.into()),
                })
            }
        }
    }

    async fn handle_input(&self, session_id: &str, data: Vec<u8>) {
        if let Some(session) = self.sessions.get(session_id) {
            let _ = session.pty_op_tx.send(PtyOp::Write(data));
        }
    }

    async fn handle_resize(&self, session_id: &str, cols: u16, rows: u16) {
        if let Some(session) = self.sessions.get(session_id) {
            let _ = session.pty_op_tx.send(PtyOp::Resize { cols, rows });
        }
    }

    async fn handle_ack(&self, session_id: &str, seq: u64, bytes_consumed: u64) {
        if let Some(session) = self.sessions.get(session_id) {
            session.ack(seq);
            session.record_ack(bytes_consumed);
        }
    }

    async fn handle_probe(&self, tool: &ToolKind) -> ProtocolMessage {
        let outcome = crate::installer::probe::probe(tool);
        ProtocolMessage::ProbeResponse {
            tool: outcome.tool,
            installed: outcome.installed,
            version: outcome.version,
            path: outcome.path,
            auth_ok: outcome.auth_ok,
            details: Some(outcome.details),
        }
    }

    async fn handle_install(
        &self,
        tool: &ToolKind,
        _version: Option<String>,
        transport_tx: &tokio::sync::mpsc::UnboundedSender<ProtocolMessage>,
    ) -> Option<ProtocolMessage> {
        let tool_clone = tool.clone();
        let tx = transport_tx.clone();

        // Spawn installation in background, emit progress events
        tokio::task::spawn_blocking(move || {
            let result = crate::installer::tool_installer::install(
                &tool_clone,
                |progress| {
                    let phase_str = match progress.phase {
                        crate::installer::tool_installer::InstallPhase::Checking => "checking",
                        crate::installer::tool_installer::InstallPhase::Downloading => "downloading",
                        crate::installer::tool_installer::InstallPhase::Installing => "installing",
                        crate::installer::tool_installer::InstallPhase::Verifying => "verifying",
                        crate::installer::tool_installer::InstallPhase::Complete => "complete",
                        crate::installer::tool_installer::InstallPhase::Failed => "failed",
                    };
                    let _ = tx.send(ProtocolMessage::InstallProgress {
                        tool: tool_clone.clone(),
                        phase: phase_str.into(),
                        progress: progress.progress,
                        message: progress.message,
                    });
                },
            );

            match result {
                Ok(version) => {
                    let _ = tx.send(ProtocolMessage::InstallComplete {
                        tool: tool_clone,
                        success: true,
                        version: Some(version),
                        error: None,
                    });
                }
                Err(e) => {
                    let _ = tx.send(ProtocolMessage::InstallComplete {
                        tool: tool_clone,
                        success: false,
                        version: None,
                        error: Some(e),
                    });
                }
            }
        });

        // Return immediately — progress comes via events
        None
    }

    pub fn shutdown(&self) {
        tracing::info!("Server shutdown initiated");
        self.registry.shutdown_all();
    }
}
