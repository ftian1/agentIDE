//! End-to-end relay test — simulates the exact code path the GUI uses:
//! spawn_session() → relay_session_output() → terminal:data events.
//!
//! This catches bugs in the relay loop, transport routing, and event
//! emission that the direct-transport tests (live_ssh_test, live_terminal_test)
//! don't cover.
//!
//! Run: SSH_HOST=... SSH_USER=... SSH_PASS=... cargo test -p remote-ai-ide --test live_relay_test -- --nocapture

use std::env;
use std::sync::Arc;
use shared_protocol::messages::ProtocolMessage;
use shared_protocol::types::ToolKind;
use remote_ai_ide_lib::transport::Transport;
use remote_ai_ide_lib::connection::ssh::{self, SshConnectionParams, AuthMethod};
fn env_or_skip(k: &str) -> String {
    env::var(k).unwrap_or_else(|_| panic!("SKIP: {} not set", k))
}

/// Full relay test: spawn_session-style loop → spawn relay → verify data arrives.
#[test]
fn test_relay_receives_terminal_data() {
    let host = env_or_skip("SSH_HOST");
    let port: u16 = env::var("SSH_PORT").unwrap_or_else(|_| "22".into()).parse().unwrap();
    let user = env_or_skip("SSH_USER");
    let pass = env_or_skip("SSH_PASS");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // 1. Connect + bootstrap (same as GUI connect command)
        let session = ssh::connect(&SshConnectionParams {
            host: host.clone(), port, user: user.clone(),
            auth: AuthMethod::Password(pass),
        }).await.expect("SSH connect");
        eprintln!("✓ connected");

        let info = remote_ai_ide_lib::bootstrap::detector::detect(&session).await.expect("detect");
        eprintln!("✓ detect: arch={} agent={:?}", info.arch, info.agent_version);

        let need = match &info.agent_version {
            Some(v) if v.trim() == "0.2.1" => false,
            _ => true,
        };
        if need {
            let bin = remote_ai_ide_lib::bootstrap::uploader::get_embedded(&info.arch).expect("no bin");
            remote_ai_ide_lib::bootstrap::uploader::upload_agent(&session, &bin, &info.home_dir)
                .await.expect("upload");
            eprintln!("✓ uploaded");
        }
        let transport: Arc<dyn Transport> = remote_ai_ide_lib::bootstrap::uploader::start_agent(&session, &info.home_dir)
            .await.expect("start agent");
        eprintln!("✓ agent started");

        // Drain Hello
        loop {
            match transport.recv().await.expect("recv") {
                Some(ProtocolMessage::Hello { .. }) => { eprintln!("✓ hello"); break; }
                None => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
                _ => {}
            }
        }

        // 2. Send SpawnSession (same as spawn_session Tauri command)
        let sid = uuid::Uuid::new_v4().to_string();
        transport.send(ProtocolMessage::SpawnSession {
            session_id: sid.clone(),
            tool: ToolKind::Custom("cat".into()),
            args: vec![],
            env: Default::default(),
            cwd: None,
            terminal_cols: 80,
            terminal_rows: 24,
            container: None,
        }).await.expect("send spawn");
        eprintln!("✓ spawn sent: cat");

        // 3. Wait for SpawnSessionAck (same as spawn_session loop)
        loop {
            match transport.recv().await.expect("recv") {
                Some(ProtocolMessage::SpawnSessionAck { session_id: ack_sid, pid, .. })
                    if ack_sid == sid =>
                {
                    eprintln!("✓ ack pid={}", pid);
                    break;
                }
                Some(ProtocolMessage::SpawnSessionNack { reason, .. }) => {
                    panic!("NACK: {}", reason);
                }
                Some(other) => { eprintln!("  (ignoring {} while waiting for ack)", other.kind()); }
                None => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
            }
        }

        // 4. Start the relay task (simulating relay_session_output)
        let t_relay = transport.clone();
        let sid_relay = sid.clone();
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

        let relay_handle = tokio::spawn(async move {
            let mut count = 0;
            loop {
                match t_relay.recv().await {
                    Ok(Some(ProtocolMessage::TerminalData { session_id: tsid, data, .. })) => {
                        if tsid != sid_relay { continue; }
                        count += 1;
                        eprintln!("  relay got TerminalData #{}: {} bytes", count, data.len());
                        let _ = event_tx.send(data);
                    }
                    Ok(Some(ProtocolMessage::CloseSessionAck { .. })) => {
                        eprintln!("  relay: session closed");
                        break;
                    }
                    Ok(None) => {
                        // No data yet — keep polling (this is the fix!)
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    }
                    Err(e) => {
                        eprintln!("  relay error: {}", e);
                        break;
                    }
                    other => {
                        eprintln!("  relay other: {:?}", other.as_ref().map(|_| "..."));
                    }
                }
            }
            eprintln!("  relay exited after {} messages", count);
        });

        // 5. Write input to cat (simulating user typing in terminal)
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let test_data = b"HELLO_RELAY_TEST_12345\n";
        transport.send(ProtocolMessage::TerminalInput {
            session_id: sid.clone(),
            data: test_data.to_vec(),
        }).await.expect("write input");
        eprintln!("✓ wrote: {}", String::from_utf8_lossy(test_data).trim());

        // 6. Wait for echoed output from cat
        let mut output: Vec<u8> = Vec::new();
        let timeout = tokio::time::sleep(std::time::Duration::from_secs(10));
        tokio::pin!(timeout);
        loop {
            tokio::select! {
                msg = event_rx.recv() => {
                    match msg {
                        Some(data) => {
                            let text = String::from_utf8_lossy(&data);
                            eprintln!("  event received: {:?}", text);
                            output.extend(&data);
                            let out_str = String::from_utf8_lossy(&output);
                            if out_str.contains("HELLO_RELAY_TEST") || output.len() > 2000 {
                                break;
                            }
                        }
                        None => break,
                    }
                }
                _ = &mut timeout => {
                    eprintln!("  (timeout waiting for relay output)");
                    break;
                }
            }
        }

        let text = String::from_utf8_lossy(&output);
        eprintln!("✓ total output: {} bytes: {:?}", output.len(), text);

        // Cleanup
        transport.send(ProtocolMessage::CloseSession { session_id: sid.clone() }).await.ok();
        relay_handle.abort();

        // Assertions
        assert!(!output.is_empty(), "Relay should have received terminal output from cat");
        assert!(text.contains("HELLO_RELAY_TEST"), "Relay output should contain the test string, got: {}", text);
        eprintln!("\n✓ Relay test PASSED — data flows correctly through relay path");
    });
}
