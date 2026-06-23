//! SSH test tool — connects to remote Linux, uploads agent binary,
//! starts it, and performs a full session lifecycle test.
//!
//! Usage: cargo run --bin ssh-test --release -- <host> <user> <password>

use anyhow::Context;
use russh::{ChannelId, client};
use russh::client::Handler;
use shared_protocol::{MessageDecoder, ProtocolMessage};
use shared_protocol::types::ToolKind;
use std::sync::{Arc, Mutex};
use std::io::Write;

// ═══════════════════════════════════════
// Shared buffer for received channel data
// ═══════════════════════════════════════

struct ChannelBuffer {
    buf: Mutex<Vec<(ChannelId, Vec<u8>)>>,
}

impl ChannelBuffer {
    fn new() -> Arc<Self> { Arc::new(Self { buf: Mutex::new(Vec::new()) }) }
    fn push(&self, ch: ChannelId, data: &[u8]) {
        self.buf.lock().unwrap().push((ch, data.to_vec()));
    }
    fn drain(&self, ch: ChannelId) -> Vec<u8> {
        let mut all = Vec::new();
        let mut rx = self.buf.lock().unwrap();
        rx.retain(|(c, data)| {
            if *c == ch { all.extend_from_slice(data); false }
            else { true }
        });
        all
    }
}

// ═══════════════════════════════════════
// SSH Handler
// ═══════════════════════════════════════

struct SshHandler { rx: Arc<ChannelBuffer> }

impl Handler for SshHandler {
    type Error = anyhow::Error;

    /// Accept all server host keys (for testing — production should verify).
    fn check_server_key(
        &mut self,
        _key: &russh::keys::PublicKey,
    ) -> impl std::future::Future<Output = Result<bool, Self::Error>> + Send {
        std::future::ready(Ok(true))
    }

    fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _session: &mut client::Session,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send {
        let rx = self.rx.clone();
        let data = data.to_vec();
        async move {
            rx.push(channel, &data);
            Ok(())
        }
    }
}

// ═══════════════════════════════════════
// Helpers
// ═══════════════════════════════════════

/// Send a protocol message on an SSH channel.
macro_rules! send_msg {
    ($ch:expr, $msg:expr) => {{
        let frame = shared_protocol::encode(&$msg)?;
        let bytes: bytes::Bytes = frame.freeze().into();
        $ch.data_bytes(bytes).await?;
    }};
}

/// Read one message from channel buffer + decoder.
fn recv_msg(rx: &ChannelBuffer, ch_id: ChannelId, dec: &mut MessageDecoder) -> Option<ProtocolMessage> {
    let data = rx.drain(ch_id);
    if !data.is_empty() { dec.push(&data); }
    match dec.try_decode() {
        Ok(Some(msg)) => Some(msg),
        _ => None,
    }
}

async fn ssh_connect(host: &str, user: &str, password: &str) -> anyhow::Result<(client::Handle<SshHandler>, Arc<ChannelBuffer>)> {
    let config = Arc::new(client::Config::default());
    let rx = ChannelBuffer::new();
    let handler = SshHandler { rx: rx.clone() };
    let addr = format!("{}:22", host);

    eprintln!("[ssh] Connecting to {}...", addr);
    let mut handle = russh::client::connect(config, &addr, handler).await?;
    let result = handle.authenticate_password(user, password).await?;
    anyhow::ensure!(result.success(), "Auth rejected");
    eprintln!("[ssh] Connected ✓");
    Ok((handle, rx))
}

/// Run a quick command, return stdout.
async fn ssh_exec(handle: &client::Handle<SshHandler>, rx: &Arc<ChannelBuffer>, cmd: &str) -> anyhow::Result<String> {
    let ch = handle.channel_open_session().await?;
    let ch_id = ch.id();
    ch.exec(true, cmd).await?;
    ch.eof().await?;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    Ok(String::from_utf8_lossy(&rx.drain(ch_id)).trim().to_string())
}

/// Upload agent binary via base64 chunks.
async fn ssh_upload(handle: &client::Handle<SshHandler>, rx: &Arc<ChannelBuffer>) -> anyhow::Result<()> {
    let bin_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/release/agent");
    let binary = std::fs::read(&bin_path).context("agent binary not found")?;
    eprintln!("[upload] {:.1} KB", binary.len() as f64 / 1024.0);

    ssh_exec(handle, rx, "mkdir -p ~/ftian/").await?;

    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&binary);
    let remote = "~/ftian/agent";

    for (i, chunk) in b64.as_bytes().chunks(65536).enumerate() {
        let s = String::from_utf8_lossy(chunk);
        let cmd = if i == 0 {
            format!("echo '{}' | base64 -d > {}", s, remote)
        } else {
            format!("echo '{}' | base64 -d >> {}", s, remote)
        };
        ssh_exec(handle, rx, &cmd).await?;
    }
    ssh_exec(handle, rx, "chmod +x ~/ftian/agent").await?;
    eprintln!("[upload] Ready ✓");
    Ok(())
}

// ═══════════════════════════════════════
// Main
// ═══════════════════════════════════════

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    anyhow::ensure!(args.len() >= 4, "Usage: ssh-test <host> <user> <password>");
    let (host, user, pw) = (&args[1], &args[2], &args[3]);

    // 1. Connect
    let (handle, rx) = ssh_connect(host, user, pw).await?;

    // 2. Detect
    let arch = ssh_exec(&handle, &rx, "uname -m").await?;
    eprintln!("[detect] arch={}", arch);

    // 3. Upload agent
    ssh_upload(&handle, &rx).await?;

    // 4. Start agent via exec channel
    let ch = handle.channel_open_session().await?;
    let ch_id = ch.id();
    ch.exec(true, "~/ftian/agent --mode stdio --log-level info").await?;
    eprintln!("[agent] Started ✓");

    // 5. Read Hello
    let mut dec = MessageDecoder::new();
    let hello = 'h: loop {
        if let Some(m) = recv_msg(&rx, ch_id, &mut dec) { break 'h m; }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    };
    match &hello {
        ProtocolMessage::Hello { version, capabilities, session_id } =>
            eprintln!("[agent] Hello v{}, caps={:?}, id={}", version, capabilities, session_id),
        other => anyhow::bail!("Expected Hello, got: {}", other.kind()),
    }

    // 6. HelloAck
    send_msg!(ch, ProtocolMessage::HelloAck { version: 1, server_version: "ssh-test".into(), server_arch: arch });

    // 7. Spawn echo
    let sid = "ssh-echo";
    eprintln!("\n═══ Spawn echo ═══");
    send_msg!(ch, ProtocolMessage::SpawnSession {
        session_id: sid.into(), tool: ToolKind::Custom("echo".into()),
        args: vec!["Hello_from_SSH_remote!!!".into()],
        env: Default::default(), cwd: None, terminal_cols: 80, terminal_rows: 24, container: None,
    });

    // 8. Wait for SpawnAck
    'ack: loop {
        match recv_msg(&rx, ch_id, &mut dec) {
            Some(ProtocolMessage::SpawnSessionAck { session_id: s, pid: p, .. }) if s == sid => {
                eprintln!("[session] pid={}", p); break 'ack;
            }
            Some(ProtocolMessage::SpawnSessionNack { reason, .. }) => anyhow::bail!("NACK: {}", reason),
            _ => tokio::time::sleep(std::time::Duration::from_millis(200)).await,
        }
    }

    // 9. Read output
    eprintln!("\n═══ Output ═══");
    loop {
        match recv_msg(&rx, ch_id, &mut dec) {
            Some(ProtocolMessage::TerminalData { session_id: s, data, seq }) if s == sid => {
                let text = String::from_utf8_lossy(&data);
                print!("{}", text); std::io::stdout().flush()?;
                send_msg!(ch, ProtocolMessage::Ack {
                    session_id: sid.into(), seq, bytes_consumed: data.len() as u64,
                });
                if text.contains("Hello_from_SSH") { break; }
            }
            Some(ProtocolMessage::SessionEvent { event_type, .. }) =>
                eprintln!("  [event: {:?}]", event_type),
            _ => tokio::time::sleep(std::time::Duration::from_millis(200)).await,
        }
    }
    eprintln!("\n═══ Verified ✓ ═══");

    // 10. Cleanup
    send_msg!(ch, ProtocolMessage::CloseSession { session_id: sid.into() });
    send_msg!(ch, ProtocolMessage::Goodbye { reason: None });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    eprintln!("Done.");
    Ok(())
}
