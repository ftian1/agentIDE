//! Live terminal interaction test — verifies full round-trip:
//! SSH → bootstrap → spawn CLI → write input → read output.
//! Run: SSH_HOST=... SSH_USER=... SSH_PASS=... cargo test -p remote-ai-ide --test live_terminal_test -- --nocapture

use std::env;
use shared_protocol::messages::ProtocolMessage;
use shared_protocol::types::ToolKind;
use remote_ai_ide_lib::transport::Transport;
use remote_ai_ide_lib::connection::ssh::{self, SshConnectionParams, AuthMethod};

fn env_or_skip(k: &str) -> String {
    env::var(k).unwrap_or_else(|_| panic!("SKIP: {} not set", k))
}

/// Spawn `cat` (echoes input back) and verify full read/write loop.
#[test]
fn test_terminal_roundtrip() {
    let host = env_or_skip("SSH_HOST");
    let port: u16 = env::var("SSH_PORT").unwrap_or_else(|_| "22".into()).parse().unwrap();
    let user = env_or_skip("SSH_USER");
    let pass = env_or_skip("SSH_PASS");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // 1. Connect + detect + start agent (skip upload if already there)
        let session = ssh::connect(&SshConnectionParams {
            host: host.clone(), port, user: user.clone(),
            auth: AuthMethod::Password(pass),
        }).await.expect("SSH connect");
        eprintln!("✓ connected");

        let info = remote_ai_ide_lib::bootstrap::detector::detect(&session).await.expect("detect");
        eprintln!("✓ detect: arch={} agent={:?}", info.arch, info.agent_version);

        let need = match &info.agent_version { Some(v) if v.trim() == "0.2.1" => false, _ => true };
        if need {
            let bin = remote_ai_ide_lib::bootstrap::uploader::get_embedded(&info.arch).expect("no bin");
            eprintln!("uploading {} KB...", bin.data.len() / 1024);
            remote_ai_ide_lib::bootstrap::uploader::upload_agent(&session, &bin, &info.home_dir)
                .await.expect("upload");
            eprintln!("✓ uploaded");
        }
        let transport = remote_ai_ide_lib::bootstrap::uploader::start_agent(&session, &info.home_dir)
            .await.expect("start agent");
        eprintln!("✓ agent started");

        // 2. Drain Hello
        loop {
            match transport.recv().await.expect("recv") {
                Some(ProtocolMessage::Hello { .. }) => { eprintln!("✓ hello"); break; }
                None => tokio::time::sleep(std::time::Duration::from_millis(200)).await,
                _ => {}
            }
        }

        // 3. Spawn `cat` (echoes stdin back to stdout)
        let sid = uuid::Uuid::new_v4().to_string();
        transport.send(ProtocolMessage::SpawnSession {
            session_id: sid.clone(), tool: ToolKind::Custom("cat".into()),
            args: vec![], env: Default::default(), cwd: None,
            terminal_cols: 80, terminal_rows: 24, container: None,
        }).await.expect("send spawn");
        eprintln!("✓ spawn sent");

        // Wait for ack
        loop {
            match transport.recv().await.expect("recv") {
                Some(ProtocolMessage::SpawnSessionAck { session_id, pid, .. }) if session_id == sid => {
                    eprintln!("✓ ack pid={}", pid); break;
                }
                Some(ProtocolMessage::SpawnSessionNack { reason, .. }) => panic!("NACK: {}", reason),
                None => tokio::time::sleep(std::time::Duration::from_millis(200)).await,
                _ => {}
            }
        }

        // 4. Write input — cat will echo it back
        let test_line = "HELLO_FROM_TEST_12345\n";
        transport.send(ProtocolMessage::TerminalInput {
            session_id: sid.clone(),
            data: test_line.as_bytes().to_vec(),
        }).await.expect("write input");
        eprintln!("✓ wrote: {}", test_line.trim());

        // 5. Read output
        let mut output = Vec::new();
        let timeout = tokio::time::sleep(std::time::Duration::from_secs(10));
        tokio::pin!(timeout);
        loop {
            tokio::select! {
                msg = transport.recv() => {
                    match msg.expect("recv") {
                        Some(ProtocolMessage::TerminalData { session_id, data, .. }) if session_id == sid => {
                            let text = String::from_utf8_lossy(&data).into_owned();
                            eprintln!("  recv: {:?}", text);
                            let done = text.contains("HELLO_FROM_TEST");
                            output.extend(data);
                            if done || output.len() > 2000 { break; }
                        }
                        Some(other) => { eprintln!("  other: {}", other.kind()); }
                        None => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
                    }
                }
                _ = &mut timeout => { eprintln!("  (timeout)"); break; }
            }
        }
        let text = String::from_utf8_lossy(&output);
        assert!(!output.is_empty(), "Expected terminal echo output");
        assert!(text.contains("HELLO_FROM_TEST"), "Expected echoed input in output, got: {}", text);
        eprintln!("✓ round-trip OK: {} bytes", output.len());
    });
}
