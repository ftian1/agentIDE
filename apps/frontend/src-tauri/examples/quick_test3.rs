use std::env;
fn env_or_skip(k: &str) -> String { env::var(k).unwrap_or_else(|_| panic!("SKIP: {}", k)) }
fn main() {
    tracing_subscriber::fmt().with_env_filter("debug").init();
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        use remote_ai_ide_lib::connection::ssh::{self, SshConnectionParams, AuthMethod};
        use remote_ai_ide_lib::transport::Transport;
        use shared_protocol::messages::ProtocolMessage;
        let s = ssh::connect(&SshConnectionParams {
            host: env_or_skip("SSH_HOST"), port: 22, user: env_or_skip("SSH_USER"),
            auth: AuthMethod::Password(env_or_skip("SSH_PASS")),
        }).await.unwrap();
        let info = remote_ai_ide_lib::bootstrap::detector::detect(&s).await.unwrap();
        let t = remote_ai_ide_lib::bootstrap::uploader::start_agent(&s, &info.home_dir).await.unwrap();
        loop { match t.recv().await.unwrap() { Some(ProtocolMessage::Hello{..})=>{break} _=>{} } }
        tracing::info!("sending...");
        t.send(ProtocolMessage::SpawnSession {
            session_id: "x".into(),
            tool: shared_protocol::types::ToolKind::Custom("echo".into()),
            args: vec!["hi".into()], env: Default::default(), cwd: None,
            terminal_cols: 80, terminal_rows: 24, container: None,
        }).await.unwrap();
        tracing::info!("sent, waiting...");
        for _ in 0..30 {
            match t.recv().await.unwrap() {
                Some(m) => { tracing::info!(kind = m.kind(), "GOT"); break; }
                None => { tokio::time::sleep(std::time::Duration::from_millis(500)).await; }
            }
        }
    });
}
