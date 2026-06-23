//! PTY Worker — spawns and manages CLI processes inside pseudo-terminals.
//!
//! Uses two separate threads to avoid deadlock:
//! - **Reader thread**: blocking reads from PTY master → TerminalData messages.
//! - **Writer thread**: receives PtyOp messages → writes to PTY master.

use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use shared_protocol::messages::ProtocolMessage;
use shared_protocol::types::*;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;

use crate::session::types::{PtyOp, Session};

const READ_CHUNK_SIZE: usize = 65536;

/// The two halves of a PTY master after splitting.
pub struct PtyHandles {
    pub reader: Box<dyn Read + Send>,
    pub writer: Box<dyn Write + Send>,
    pub pid: u32,
}

/// Spawn a CLI process in a PTY.
pub fn spawn_cli(
    command: &str,
    args: &[String],
    env: &HashMap<String, String>,
    cwd: Option<&str>,
    cols: u16,
    rows: u16,
) -> anyhow::Result<PtyHandles> {
    let pty_system = NativePtySystem::default();
    let size = PtySize { rows, cols, pixel_width: 0, pixel_height: 0 };
    let pty_pair = pty_system.openpty(size)?;

    let mut cmd = CommandBuilder::new(command);
    cmd.args(args);

    for (key, value) in env {
        cmd.env(key, value);
    }
    if !env.contains_key("PATH") {
        if let Ok(p) = std::env::var("PATH") { cmd.env("PATH", p); }
    }
    if !env.contains_key("HOME") {
        if let Ok(h) = std::env::var("HOME") { cmd.env("HOME", h); }
    }
    cmd.env("TERM", "xterm-256color");
    if let Some(dir) = cwd { cmd.cwd(dir); }

    let child = pty_pair.slave.spawn_command(cmd)?;
    let pid = child.process_id().unwrap_or(0);
    drop(child); // PTY master keeps the process alive

    let reader = pty_pair.master.try_clone_reader()
        .map_err(|e| anyhow::anyhow!("try_clone_reader: {}", e))?;
    let writer = pty_pair.master.take_writer()
        .map_err(|e| anyhow::anyhow!("take_writer: {}", e))?;

    tracing::info!(command = %command, pid = pid, cols = cols, rows = rows, "Spawned CLI");

    Ok(PtyHandles { reader, writer, pid })
}

/// Start the PTY reader + writer threads.
///
/// Returns `JoinHandle`s for both threads.
/// The reader pushes `TerminalData` messages; the writer processes `PtyOp`s.
pub fn run_pty_loop(
    session: Arc<Session>,
    handles: PtyHandles,
    transport_tx: tokio::sync::mpsc::UnboundedSender<ProtocolMessage>,
    mut write_rx: tokio::sync::mpsc::UnboundedReceiver<PtyOp>,
    registry: Arc<crate::session::registry::SessionRegistry>,
) -> (std::thread::JoinHandle<()>, std::thread::JoinHandle<()>) {
    let PtyHandles { mut reader, mut writer, pid: _ } = handles;
    let session_id = session.id.clone();

    // ── Writer thread ─────────────────────────────────
    let write_handle = std::thread::spawn(move || {
        loop {
            match write_rx.blocking_recv() {
                Some(PtyOp::Write(data)) => {
                    if writer.write_all(&data).is_err() {
                        break;
                    }
                    let _ = writer.flush();
                }
                Some(PtyOp::Resize { .. }) => {
                    // PTY resize handled by the PTY system
                }
                Some(PtyOp::Shutdown) | None => break,
            }
        }
    });

    // ── Reader thread ─────────────────────────────────
    let reader_session = session.clone();
    let reader_registry = registry.clone();
    let sid = session_id.clone();

    // Agent-stream parsing: only agent CLIs emit stream-json worth parsing.
    // Bash/shell sessions produce raw terminal output only.
    let parse_agent = matches!(session.tool, ToolKind::Claude | ToolKind::Custom(_));
    let parse_tx = transport_tx.clone();

    let read_handle = std::thread::spawn(move || {
        let mut buf = vec![0u8; READ_CHUNK_SIZE];
        let mut parser = parse_agent.then(|| crate::agent_parse::AgentStreamParser::new(sid.clone()));
        let mut line_buf: Vec<u8> = Vec::new();

        loop {
            // Flow control check
            while reader_session.paused.load(std::sync::atomic::Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }

            match reader.read(&mut buf) {
                Ok(0) => {
                    tracing::info!(session_id = %sid, "PTY EOF");
                    break;
                }
                Ok(n) => {
                    let data = buf[..n].to_vec();
                    let seq = reader_session.next_seq();
                    let bytes = data.len();
                    reader_session.record_sent(bytes);

                    // Feed the agent-stream parser line-by-line (newline-delimited
                    // JSON). Raw terminal bytes still flow through unchanged.
                    if let Some(parser) = parser.as_mut() {
                        line_buf.extend_from_slice(&data);
                        while let Some(pos) = line_buf.iter().position(|&b| b == b'\n') {
                            let line: Vec<u8> = line_buf.drain(..=pos).collect();
                            if let Ok(text) = std::str::from_utf8(&line) {
                                for msg in parser.push_line(text) {
                                    let _ = parse_tx.send(msg);
                                }
                            }
                        }
                        // Bound the partial-line buffer to avoid unbounded growth
                        // on a stream that never emits a newline.
                        if line_buf.len() > 1_048_576 {
                            line_buf.clear();
                        }
                    }

                    let msg = ProtocolMessage::TerminalData {
                        session_id: sid.clone(),
                        data,
                        seq,
                    };

                    if transport_tx.send(msg).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::info!(session_id = %sid, error = %e, "PTY read error");
                    break;
                }
            }
        }

        // Mark session as ended
        if let Some(sess) = reader_registry.get(&sid) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut state = sess.state.write().await;
                if !matches!(*state, SessionState::Ended(_)) {
                    *state = SessionState::Ended(EndReason::ProcessExited(0));
                }
            });
        }
    });

    (read_handle, write_handle)
}
