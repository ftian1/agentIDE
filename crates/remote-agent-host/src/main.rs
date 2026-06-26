//! Remote Agent Host — entry point.
//!
//! Reads protocol messages from stdin and writes responses to stdout.
//! Used with SSH exec channel: `ssh user@host -- ~/.remote-agent-host/agent --mode stdio`

use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

use tap::UpstreamProxy;

mod server;
mod session;
mod pty;
mod transport;
mod installer;
mod agent_parse;
mod tap;

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

/// Extract --test-tap <mitm|reverse> from argv, if present.
fn parse_test_tap_mode() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--test-tap" && i + 1 < args.len() {
            return Some(args[i + 1].clone());
        }
        i += 1;
    }
    None
}

/// Extract --test-tap-upstream <host> from argv (for reverse mode).
fn parse_test_tap_upstream() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--test-tap-upstream" && i + 1 < args.len() {
            return Some(args[i + 1].clone());
        }
        i += 1;
    }
    None
}

/// Extract --test-tap-proxy <url> from argv (corporate forward-proxy).
fn parse_test_tap_proxy() -> Option<UpstreamProxy> {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--test-tap-proxy" && i + 1 < args.len() {
            return tap::parse_upstream_proxy(&args[i + 1]);
        }
        i += 1;
    }
    // Also check env
    std::env::var("HTTPS_PROXY")
        .ok()
        .or_else(|| std::env::var("https_proxy").ok())
        .and_then(|v| tap::parse_upstream_proxy(&v))
}

/// Standalone tap test mode: starts a proxy, prints its port + CA path, then
/// collects captured exchanges for `timeout_secs` seconds, printing each as
/// JSON to stdout before exiting.
async fn run_tap_test(tap_mode: &str, upstream: Option<String>, timeout_secs: u64) -> anyhow::Result<()> {
    use shared_protocol::messages::ProtocolMessage;
    use shared_protocol::types::TapMode;

    let mode = match tap_mode {
        "reverse" => TapMode::Reverse,
        _ => TapMode::Mitm,
    };
    let upstream_host = upstream.or_else(|| {
        if matches!(mode, TapMode::Reverse) {
            Some("api.anthropic.com".to_string())
        } else {
            None
        }
    });

    let upstream_proxy = parse_test_tap_proxy();

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ProtocolMessage>();

    let handle = tap::proxy::start_session_proxy(
        "test-session".to_string(),
        tx.clone(),
        mode.clone(),
        upstream_host.clone(),
        upstream_proxy.clone(),
        None, // gateway_provider
        None, // gateway_token
        None, // gateway_mode
        None, // gateway_path_prefix
        Vec::new(), // providers (no model-based routing in test mode)
    )?;

    let proxy_label = upstream_proxy
        .as_ref()
        .map(|p| format!("{}:{}", p.host, p.port))
        .unwrap_or_else(|| "none".to_string());
    eprintln!(
        "TAP_TEST_READY|port={}|mode={}|ca_pem={}|upstream={}|upstream_proxy={}",
        handle.port,
        tap_mode,
        handle.ca_pem_path.display(),
        upstream_host.as_deref().unwrap_or("n/a"),
        proxy_label,
    );

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    let mut exchange_count = 0u64;

    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(msg)) => {
                if let ProtocolMessage::HttpTraffic { session_id: _, exchange, seq: _ } = msg {
                    if let Ok(json) = serde_json::to_string(&exchange) {
                        println!("{}", json);
                    }
                    exchange_count += 1;
                }
            }
            Ok(None) => break,
            Err(_) => {
                eprintln!("TAP_TEST_TIMEOUT|exchanges_captured={}", exchange_count);
                break;
            }
        }
    }

    drop(handle); // stops proxy accept loop
    Ok(())
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

    // Parse --log-file <path> and --log-level <level> for persistent debug logging
    let (log_file, log_level) = {
        let args: Vec<String> = std::env::args().collect();
        let mut log_path = None;
        let mut level = "info".to_string();
        let mut i = 1;
        while i < args.len() {
            if args[i] == "--log-file" && i + 1 < args.len() {
                log_path = Some(args[i + 1].clone());
                i += 1;
            } else if args[i] == "--log-level" && i + 1 < args.len() {
                level = args[i + 1].clone();
                i += 1;
            }
            i += 1;
        }
        (log_path, level)
    };

    // Dual-writer: writes to both stderr (SSH channel) and a file.
    struct DualWriter {
        file: std::sync::Mutex<std::fs::File>,
    }
    impl std::io::Write for DualWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let _ = std::io::stderr().write_all(buf);
            self.file.lock().unwrap().write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            let _ = std::io::stderr().flush();
            self.file.lock().unwrap().flush()
        }
    }

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&log_level));

    if let Some(path) = log_file {
        match std::fs::File::create(&path) {
            Ok(file) => {
                tracing_subscriber::fmt()
                    .with_writer(std::sync::Mutex::new(DualWriter { file: std::sync::Mutex::new(file) }))
                    .with_env_filter(env_filter)
                    .with_target(false)
                    .with_ansi(false)
                    .init();
            }
            Err(_) => {
                tracing_subscriber::fmt()
                    .with_writer(std::io::stderr)
                    .with_env_filter(env_filter)
                    .with_target(false)
                    .with_ansi(false)
                    .init();
            }
        }
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
                    .with_env_filter(env_filter)
                    .with_target(false)
                    .with_ansi(false)
                    .init();
            }
            Err(_) => {
                tracing_subscriber::fmt()
                    .with_writer(std::io::stderr)
                    .with_env_filter(env_filter)
                    .with_target(false)
                    .with_ansi(false)
                    .init();
            }
        }
    }

    // --test-tap: standalone proxy test (exits after the test, no stdio loop)
    if let Some(tap_mode) = parse_test_tap_mode() {
        let upstream = parse_test_tap_upstream();
        let timeout = std::env::var("TAP_TEST_TIMEOUT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);
        run_tap_test(&tap_mode, upstream, timeout).await?;
        return Ok(());
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
