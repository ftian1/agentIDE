//! Live SSH integration test.
//! Run: SSH_HOST=... SSH_USER=... SSH_PASS=... cargo test -p remote-ai-ide --test live_ssh_test -- --nocapture

use std::env;

fn env_or_skip(key: &str) -> String {
    env::var(key).unwrap_or_else(|_| panic!("SKIP: {} not set", key))
}

#[test]
fn test_ssh_connect_and_detect() {
    let host = env_or_skip("SSH_HOST");
    let port: u16 = env::var("SSH_PORT").unwrap_or_else(|_| "22".into()).parse().unwrap();
    let user = env_or_skip("SSH_USER");
    let pass = env_or_skip("SSH_PASS");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        use remote_ai_ide_lib::connection::ssh::{self, SshConnectionParams, AuthMethod};

        let session = ssh::connect(&SshConnectionParams {
            host: host.clone(), port, user: user.clone(),
            auth: AuthMethod::Password(pass),
        }).await.expect("SSH connect failed");
        eprintln!("✓ connected to {}:{}", host, port);

        let user_out = ssh::exec_remote(&session, "whoami").await.expect("whoami");
        assert_eq!(user_out.trim(), user);
        eprintln!("✓ whoami={}", user_out.trim());

        let info = remote_ai_ide_lib::bootstrap::detector::detect(&session).await
            .expect("detect failed");
        eprintln!("✓ arch={} platform={} home={}", info.arch, info.platform, info.home_dir);
    });
}

#[test]
fn test_full_upload_start_spawn() {
    let host = env_or_skip("SSH_HOST");
    let port: u16 = env::var("SSH_PORT").unwrap_or_else(|_| "22".into()).parse().unwrap();
    let user = env_or_skip("SSH_USER");
    let pass = env_or_skip("SSH_PASS");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        use remote_ai_ide_lib::connection::ssh::{self, SshConnectionParams, AuthMethod};
        use remote_ai_ide_lib::transport::Transport;
        use shared_protocol::messages::ProtocolMessage;
        use shared_protocol::types::ToolKind;

        let session = ssh::connect(&SshConnectionParams {
            host, port, user: user.clone(),
            auth: AuthMethod::Password(pass),
        }).await.expect("SSH connect");
        eprintln!("✓ connected");

        let info = remote_ai_ide_lib::bootstrap::detector::detect(&session).await.expect("detect");
        eprintln!("✓ detect: arch={} agent={:?}", info.arch, info.agent_version);

        // Version check: --version outputs just "0.2.1"
        let need = match &info.agent_version {
            Some(v) if v.trim() == "0.2.1" => false,
            _ => true,
        };
        if need {
            let bin = remote_ai_ide_lib::bootstrap::uploader::get_embedded(&info.arch)
                .expect("no binary");
            eprintln!("uploading {} KB...", bin.data.len() / 1024);
            remote_ai_ide_lib::bootstrap::uploader::upload_agent(&session, &bin, &info.home_dir)
                .await.expect("upload failed");
            eprintln!("✓ uploaded");
        } else {
            eprintln!("✓ skip upload (v{})", info.agent_version.as_ref().unwrap());
        }

        let transport = remote_ai_ide_lib::bootstrap::uploader::start_agent(&session, &info.home_dir)
            .await.expect("start_agent");
        eprintln!("✓ agent started");

        let hello = loop {
            match transport.recv().await.expect("recv") {
                Some(msg) => { eprintln!("✓ hello: {}", msg.kind()); break msg; }
                None => tokio::time::sleep(std::time::Duration::from_millis(200)).await,
            }
        };

        let sid = uuid::Uuid::new_v4().to_string();
        transport.send(ProtocolMessage::SpawnSession {
            session_id: sid.clone(), tool: ToolKind::Custom("echo".into()),
            args: vec!["LIVE_TEST_OK".into()], env: Default::default(), cwd: None,
            terminal_cols: 80, terminal_rows: 24, container: None,
        }).await.expect("send spawn");
        eprintln!("✓ spawn sent");

        // Collect all messages: SpawnAck + TerminalData may arrive interleaved
        let mut out = Vec::new();
        let mut acked = false;
        for _ in 0..300 {
            match transport.recv().await.expect("recv") {
                Some(ProtocolMessage::SpawnSessionAck { session_id, pid, .. }) if session_id == sid => {
                    eprintln!("✓ ack pid={}", pid);
                    acked = true;
                    if !out.is_empty() { break; } // got both ack + output
                }
                Some(ProtocolMessage::SpawnSessionNack { reason, .. }) => panic!("NACK: {}", reason),
                Some(ProtocolMessage::TerminalData { data, .. }) => {
                    eprintln!("  data {} bytes", data.len());
                    out.extend(&data);
                    if acked && out.contains(&b'\n') { break; }
                }
                Some(ProtocolMessage::CloseSessionAck { .. }) => {
                    eprintln!("  session closed"); break;
                }
                Some(other) => { eprintln!("  other: {}", other.kind()); }
                None => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
            }
        }
        let text = String::from_utf8_lossy(&out);
        eprintln!("✓ output ({} bytes): {:?}", out.len(), text);
        assert!(acked, "Did not receive SpawnSessionAck");
        assert!(!out.is_empty(), "Expected terminal output");
        assert!(text.contains("LIVE_TEST_OK"), "Expected LIVE_TEST_OK");
    });
}
