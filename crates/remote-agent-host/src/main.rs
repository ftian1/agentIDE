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
    // CRITICAL: logging must go to stderr, never stdout.
    // stdout is the protocol wire — any extra bytes corrupt framing.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .with_ansi(false) // No ANSI escapes in stderr either (cleaner logs)
        .init();

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
