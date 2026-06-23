//! Remote Agent Host — entry point.
//!
//! Reads protocol messages from stdin and writes responses to stdout.
//! Used with SSH exec channel: `ssh user@host -- ~/.remote-agent-host/agent --mode stdio`

use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

mod server;
mod session;
mod pty;
mod transport;
mod installer;
mod agent_parse;

fn parse_mode() -> String {
    let args: Vec<String> = std::env::args().collect();
    let mut mode = "stdio".to_string();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--mode" && i + 1 < args.len() {
            mode = args[i + 1].clone();
        }
        i += 1;
    }
    mode
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Handle --version: print version and exit (used by bootstrap for version check)
    for arg in std::env::args() {
        if arg == "--version" {
            println!("0.2.1");
            return Ok(());
        }
    }

    // Parse --log-file <path> for persistent debug logging
    let log_file = {
        let args: Vec<String> = std::env::args().collect();
        let mut log_path = None;
        let mut i = 1;
        while i < args.len() {
            if args[i] == "--log-file" && i + 1 < args.len() {
                log_path = Some(args[i + 1].clone());
                i += 1;
            }
            i += 1;
        }
        log_path
    };

    // Set up logging: always to stderr, optionally also to a file.
    let file_log = log_file.clone();
    if let Some(path) = file_log {
        let file = std::fs::File::create(&path)
            .unwrap_or_else(|_| panic!("Failed to create log file: {}", path));
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_env_filter(
                EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| EnvFilter::new("info")),
            )
            .with_target(false)
            .with_ansi(false)
            .init();
        // Also write a marker to the file so we know logging works
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(&path)
            .unwrap_or_else(|_| panic!("Failed to open log file: {}", path));
        let _ = writeln!(f, "Agent starting pid={}", std::process::id());
    } else {
        // Fallback: write log to ~/.remote-agent-host/agent.log
        let default_log = std::env::var("HOME")
            .map(|h| std::path::PathBuf::from(h).join(".remote-agent-host/agent.log"))
            .unwrap_or_else(|_| std::path::PathBuf::from("/tmp/remote-agent-host-agent.log"));
        let _ = std::fs::create_dir_all(default_log.parent().unwrap());
        match std::fs::File::create(&default_log) {
            Ok(file) => {
                tracing_subscriber::fmt()
                    .with_writer(std::sync::Mutex::new(file))
                    .with_env_filter(
                        EnvFilter::try_from_default_env()
                            .unwrap_or_else(|_| EnvFilter::new("info")),
                    )
                    .with_target(false)
                    .with_ansi(false)
                    .init();
            }
            Err(_) => {
                // Can't create log file — fall back to stderr only
                tracing_subscriber::fmt()
                    .with_writer(std::io::stderr)
                    .with_env_filter(
                        EnvFilter::try_from_default_env()
                            .unwrap_or_else(|_| EnvFilter::new("info")),
                    )
                    .with_target(false)
                    .with_ansi(false)
                    .init();
            }
        }
    }

    let _mode = parse_mode();
    tracing::info!("Remote Agent Host starting");

    let mut server = server::Server::new();
    let host_id = server.host_id.clone();

    // Send Hello
    let hello = shared_protocol::ProtocolMessage::Hello {
        version: 1,
        capabilities: vec!["pty".into(), "probe".into(), "install".into(), "flow_control".into()],
        session_id: host_id,
    };
    transport::stdio::write_message(&hello).await?;
    tracing::info!("Sent Hello");

    // Set up channels
    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
    let (resp_tx, mut resp_rx) = mpsc::unbounded_channel::<shared_protocol::ProtocolMessage>();

    // Spawn stdin reader — pushes messages into msg_tx
    let _read_handle = tokio::spawn(async move {
        if let Err(e) = transport::stdio::read_loop(msg_tx).await {
            tracing::error!(error = %e, "Read loop ended");
        }
    });

    // Main dispatch loop
    loop {
        tokio::select! {
            msg = msg_rx.recv() => {
                match msg {
                    Some(msg) => {
                        if matches!(msg, shared_protocol::ProtocolMessage::Goodbye { .. }) {
                            tracing::info!("Goodbye received");
                            break;
                        }
                        let response = server.dispatch(msg, &resp_tx).await;
                        if let Some(resp) = response {
                            transport::stdio::write_message(&resp).await?;
                        }
                    }
                    None => {
                        tracing::info!("Message channel closed — stdin EOF");
                        break;
                    }
                }
            }
            Some(resp) = resp_rx.recv() => {
                transport::stdio::write_message(&resp).await?;
            }
        }
    }

    server.shutdown();
    tracing::info!("Remote Agent Host stopped");
    Ok(())
}
