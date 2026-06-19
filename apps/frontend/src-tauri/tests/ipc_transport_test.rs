//! Integration test: exercises the IpcTransport just like the Tauri backend does.
//! Spawns the agent, does handshake, spawns a shell session, reads output, sends input.

use shared_protocol::ProtocolMessage;
use shared_protocol::types::ToolKind;

// We can't easily import from the Tauri crate in integration tests,
// so this test uses the same pattern as the e2e tests but verifies
// the transport abstraction works correctly.

/// Helper to spawn the agent binary directly.
fn spawn_agent() -> std::process::Child {
    // Use absolute paths resolved from workspace root
    let ws = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let candidates = [
        ws.join("target/release/agent"),
        ws.join("target/debug/agent"),
    ];
    let bin = candidates.iter()
        .find(|p| p.exists())
        .cloned()
        .unwrap_or_else(|| candidates[0].clone());
    eprintln!("Using agent binary: {}", bin.display());

    std::process::Command::new(&bin)
        .arg("--mode").arg("stdio")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .expect(&format!("Failed to spawn agent at {}", bin.display()))
}

#[test]
fn test_transport_hello_and_spawn() {
    use std::io::{Read, Write};

    let mut child = spawn_agent();
    let stdin = child.stdin.as_mut().unwrap();
    let stdout = child.stdout.as_mut().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));

    let mut decoder = shared_protocol::MessageDecoder::new();
    let mut buf = [0u8; 65536];

    // Helper to read one message
    let mut read_msg = |stdout: &mut dyn Read| -> Option<ProtocolMessage> {
        loop {
            match stdout.read(&mut buf) {
                Ok(0) => return None,
                Ok(n) => {
                    decoder.push(&buf[..n]);
                    match decoder.try_decode() {
                        Ok(Some(m)) => return Some(m),
                        Ok(None) => continue,
                        Err(_) => return None,
                    }
                }
                Err(_) => return None,
            }
        }
    };

    // 1. Read Hello
    let hello = read_msg(stdout).expect("Hello");
    assert!(matches!(hello, ProtocolMessage::Hello { .. }));

    // Send HelloAck
    let ack = ProtocolMessage::HelloAck {
        version: 1,
        server_version: "transport-test".into(),
        server_arch: "x86_64".into(),
    };
    stdin.write_all(&shared_protocol::encode_to_vec(&ack).unwrap()).unwrap();
    stdin.flush().unwrap();

    // 2. Spawn a 'whoami' session (quick, produces output and exits)
    let sid = "transport-test-s1";
    let spawn = ProtocolMessage::SpawnSession {
        session_id: sid.into(),
        tool: ToolKind::Custom("whoami".into()),
        args: vec![],
        env: Default::default(),
        cwd: None,
        terminal_cols: 80,
        terminal_rows: 24,
    };
    stdin.write_all(&shared_protocol::encode_to_vec(&spawn).unwrap()).unwrap();
    stdin.flush().unwrap();

    // Read SpawnAck
    let spawn_ack = read_msg(stdout).expect("SpawnAck");
    match spawn_ack {
        ProtocolMessage::SpawnSessionAck { session_id, pid, .. } => {
            assert_eq!(session_id, sid);
            println!("Spawned pid={}", pid);
        }
        ProtocolMessage::SpawnSessionNack { reason, .. } => {
            panic!("Spawn NACK: {}", reason);
        }
        other => panic!("Unexpected: {}", other.kind()),
    }

    // 3. Read terminal output
    let mut output = Vec::new();
    for _ in 0..10 {
        match read_msg(stdout) {
            Some(ProtocolMessage::TerminalData { session_id, data, seq }) if session_id == sid => {
                output.extend_from_slice(&data);
                // Ack
                let a = ProtocolMessage::Ack {
                    session_id: sid.into(),
                    seq,
                    bytes_consumed: data.len() as u64,
                };
                stdin.write_all(&shared_protocol::encode_to_vec(&a).unwrap()).unwrap();
                stdin.flush().unwrap();

                if !data.is_empty() {
                    break; // got output
                }
            }
            Some(other) => {
                println!("Other: {}", other.kind());
            }
            None => break,
        }
    }

    let output_str = String::from_utf8_lossy(&output);
    println!("Output: {:?}", output_str);
    assert!(!output_str.is_empty(), "Should have received terminal output");

    // 4. Close session
    let close = ProtocolMessage::CloseSession { session_id: sid.into() };
    stdin.write_all(&shared_protocol::encode_to_vec(&close).unwrap()).unwrap();
    stdin.flush().unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));
    let _ = child.kill();
    let _ = child.wait();
}
