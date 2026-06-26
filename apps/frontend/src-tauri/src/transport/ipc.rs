//! IPC Transport — spawns the Remote Agent Host as a local child process
//! and communicates via stdin/stdout pipes.
//!
//! This enables full end-to-end testing without a remote Linux machine or SSH.
//! The agent binary is resolved from the Tauri resource directory or PATH.
//!
//! Wire format is the standard length-prefixed MessagePack protocol.

use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

use async_trait::async_trait;
use shared_protocol::{MessageDecoder, ProtocolMessage};

use super::Transport;

/// An [`Transport`] that talks to a locally-spawned agent process via pipes.
pub struct IpcTransport {
    /// Channel to send messages to the writer thread.
    write_tx: mpsc::UnboundedSender<Vec<u8>>,
    /// Channel to receive messages from the reader thread.
    read_rx: tokio::sync::Mutex<mpsc::UnboundedReceiver<ProtocolMessage>>,
    /// Connected flag.
    connected: Arc<AtomicBool>,
    /// Child process handle (kept alive until drop).
    _child: Option<Child>,
    /// Reader thread handle.
    _reader: Option<std::thread::JoinHandle<()>>,
    /// Writer thread handle.
    _writer: Option<std::thread::JoinHandle<()>>,
}

impl IpcTransport {
    /// Start the agent binary as a child process and set up pipe I/O threads.
    ///
    /// The agent binary is looked up in this order:
    /// 1. `binaries/agent` relative to the executable directory
    /// 2. `agent` in the current working directory
    /// 3. `agent` on PATH (searching `target/release/`, `target/debug/`, and system PATH)
    pub fn spawn() -> anyhow::Result<Self> {
        let binary_path = Self::find_agent_binary()?;
        tracing::info!(path = %binary_path.display(), "Starting agent via IPC");

        // Build a default log path so the agent always writes to a file,
        // matching the SSH bootstrap behaviour (uploader.rs start_agent).
        let agent_log = std::env::var("HOME")
            .map(|h| format!("{}/.remote-agent-host/agent.log", h.trim_end_matches('/')))
            .unwrap_or_else(|_| "/tmp/remote-agent-host-agent.log".to_string());
        let mut child = Command::new(&binary_path)
            .arg("--mode").arg("stdio")
            .arg("--log-level").arg("debug")
            .arg("--log-file").arg(&agent_log)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to spawn agent: {}", e))?;

        let mut stdin = child.stdin.take()
            .ok_or_else(|| anyhow::anyhow!("No stdin pipe"))?;
        let stdout = child.stdout.take()
            .ok_or_else(|| anyhow::anyhow!("No stdout pipe"))?;

        let connected = Arc::new(AtomicBool::new(true));
        let conn_write = connected.clone();

        // Channel: writer thread receives bytes, writes to stdin
        let (write_tx, mut write_rx) = mpsc::unbounded_channel::<Vec<u8>>();

        // Channel: reader thread reads from stdout, sends decoded messages
        let (read_tx, read_rx) = mpsc::unbounded_channel::<ProtocolMessage>();

        // Spawn writer thread
        let writer = std::thread::spawn(move || {
            while conn_write.load(Ordering::SeqCst) {
                match write_rx.blocking_recv() {
                    Some(data) => {
                        if stdin.write_all(&data).is_err() { break; }
                        let _ = stdin.flush();
                    }
                    None => break,
                }
            }
        });

        // Spawn reader thread
        let conn_read = connected.clone();
        let reader = std::thread::spawn(move || {
            let mut stdout = stdout;
            let mut decoder = MessageDecoder::new();
            let mut buf = [0u8; 65536];

            while conn_read.load(Ordering::SeqCst) {
                match stdout.read(&mut buf) {
                    Ok(0) => break, // EOF — agent exited
                    Ok(n) => {
                        decoder.push(&buf[..n]);
                        loop {
                            match decoder.try_decode() {
                                Ok(Some(msg)) => {
                                    if read_tx.send(msg).is_err() { return; }
                                }
                                Ok(None) => break, // need more data
                                Err(e) => {
                                    tracing::error!(error = %e, "IPC decode error");
                                    return;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "IPC read error");
                        break;
                    }
                }
            }
            conn_read.store(false, Ordering::SeqCst);
        });

        tracing::info!("Agent process started via IPC");

        Ok(Self {
            write_tx,
            read_rx: tokio::sync::Mutex::new(read_rx),
            connected,
            _child: Some(child),
            _reader: Some(reader),
            _writer: Some(writer),
        })
    }

    /// Find the agent binary by searching common locations.
    fn find_agent_binary() -> anyhow::Result<std::path::PathBuf> {
        // 1. Check relative to the current executable
        if let Ok(exe) = std::env::current_exe() {
            let beside = exe.parent().unwrap_or(std::path::Path::new("."))
                .join("binaries").join("agent");
            if beside.exists() {
                return Ok(beside);
            }
        }

        // 2. Check cargo target directories (dev convenience)
        let cargo_paths = [
            "target/release/agent",
            "target/debug/agent",
            "../../target/release/agent",  // from apps/frontend/src-tauri/
            "../../../target/release/agent", // from apps/frontend/
        ];
        for p in &cargo_paths {
            let path = std::path::PathBuf::from(p);
            if path.exists() { return Ok(path.canonicalize()?); }
        }

        // 3. Check PATH
        if let Ok(path) = which::which("agent") {
            return Ok(path);
        }

        anyhow::bail!(
            "Agent binary not found. Build with: cargo build -p remote-agent-host --release"
        )
    }
}

#[async_trait]
impl Transport for IpcTransport {
    async fn send(&self, msg: ProtocolMessage) -> anyhow::Result<()> {
        if !self.connected.load(Ordering::SeqCst) {
            anyhow::bail!("IPC transport disconnected");
        }
        let frame = shared_protocol::encode(&msg)?;
        self.write_tx.send(frame.to_vec())
            .map_err(|_| anyhow::anyhow!("Writer thread disconnected"))?;
        Ok(())
    }

    async fn recv(&self) -> anyhow::Result<Option<ProtocolMessage>> {
        let mut rx = self.read_rx.lock().await;
        match rx.try_recv() {
            Ok(msg) => Ok(Some(msg)),
            Err(mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(mpsc::error::TryRecvError::Disconnected) => Ok(None),
        }
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    async fn close(&self) -> anyhow::Result<()> {
        self.connected.store(false, Ordering::SeqCst);
        Ok(())
    }
}
