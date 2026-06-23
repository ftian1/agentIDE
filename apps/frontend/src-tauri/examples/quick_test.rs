// Minimal test: connect → start agent → hello → send spawn → receive ack
use std::env;
fn env_or_skip(k: &str) -> String { env::var(k).unwrap_or_else(|_| panic!("SKIP: {}", k)) }
fn main() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        use remote_ai_ide_lib::connection::ssh::{self, SshConnectionParams, AuthMethod};
        use remote_ai_ide_lib::transport::Transport;
        use shared_protocol::messages::ProtocolMessage;
        use shared_protocol::types::ToolKind;
        let s = ssh::connect(&SshConnectionParams {
            host: env_or_skip("SSH_HOST"), port: 22, user: env_or_skip("SSH_USER"),
            auth: AuthMethod::Password(env_or_skip("SSH_PASS")),
        }).await.unwrap();
        let info = remote_ai_ide_lib::bootstrap::detector::detect(&s).await.unwrap();
        let t = remote_ai_ide_lib::bootstrap::uploader::start_agent(&s, &info.home_dir).await.unwrap();
        // Drain hello
        loop { match t.recv().await.unwrap() { Some(ProtocolMessage::Hello{..})=>{break} _=>{} } }
        let sid = uuid::Uuid::new_v4().to_string();
        t.send(ProtocolMessage::SpawnSession { session_id:sid.clone(), tool:ToolKind::Custom("echo".into()), args:vec!["TEST_OK".into()], env:Default::default(), cwd:None, terminal_cols:80, terminal_rows:24, container:None }).await.unwrap();
        eprintln!("sent spawn");
        for _ in 0..100 {
            match t.recv().await.unwrap() {
                Some(m) => { eprintln!("GOT: {}", m.kind()); break; }
                None => tokio::time::sleep(std::time::Duration::from_millis(200)).await,
            }
        }
    });
}
