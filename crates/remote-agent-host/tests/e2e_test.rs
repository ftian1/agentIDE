//! End-to-end integration tests for the Remote Agent Host protocol.
//!
//! Spawns the agent as a child process and communicates over stdio.

use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

const AGENT_BIN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/test-agent");

fn encode_frame(msg: &shared_protocol::ProtocolMessage) -> Vec<u8> {
    shared_protocol::encode_to_vec(msg).expect("encode failed")
}

fn read_message<R: Read>(reader: &mut R) -> Option<shared_protocol::ProtocolMessage> {
    let mut decoder = shared_protocol::MessageDecoder::new();
    let mut buf = [0u8; 65536];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => return None,
            Ok(n) => {
                decoder.push(&buf[..n]);
                match decoder.try_decode() {
                    Ok(Some(msg)) => return Some(msg),
                    Ok(None) => continue,
                    Err(e) => {
                        eprintln!("Decode error: {}", e);
                        return None;
                    }
                }
            }
            Err(e) => {
                eprintln!("Read error: {}", e);
                return None;
            }
        }
    }
}

struct AgentProcess { child: Child }

impl AgentProcess {
    fn start() -> Self {
        let child = Command::new(AGENT_BIN)
            .arg("--mode").arg("stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("Failed to start agent");
        std::thread::sleep(Duration::from_millis(200));
        Self { child }
    }

    fn stdin(&mut self) -> &mut dyn Write {
        self.child.stdin.as_mut().expect("no stdin")
    }

    fn stdout(&mut self) -> &mut dyn Read {
        self.child.stdout.as_mut().expect("no stdout")
    }

    fn send(&mut self, msg: &shared_protocol::ProtocolMessage) {
        let frame = encode_frame(msg);
        self.stdin().write_all(&frame).expect("write failed");
        self.stdin().flush().expect("flush failed");
    }

    fn recv(&mut self) -> Option<shared_protocol::ProtocolMessage> {
        read_message(&mut self.stdout())
    }

    /// Do the hello handshake.
    fn handshake(&mut self) {
        let hello = self.recv().expect("Expected Hello");
        assert!(matches!(hello, shared_protocol::ProtocolMessage::Hello { .. }));
        self.send(&shared_protocol::ProtocolMessage::HelloAck {
            version: 1,
            server_version: "test".into(),
            server_arch: "x86_64".into(),
        });
    }
}

impl Drop for AgentProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn test_hello_handshake() {
    let mut agent = AgentProcess::start();
    agent.handshake();
    eprintln!("✓ Hello handshake OK");
}

#[test]
fn test_spawn_session_and_read_output() {
    let mut agent = AgentProcess::start();
    agent.handshake();

    let sid = "test-s1";
    agent.send(&shared_protocol::ProtocolMessage::SpawnSession {
        session_id: sid.into(),
        tool: shared_protocol::types::ToolKind::Custom("echo".into()),
        args: vec!["hello_from_pty".into()],
        env: Default::default(),
        cwd: None,
        terminal_cols: 80,
        terminal_rows: 24,
        container: None,
    });

    // Read SpawnSessionAck
    let ack = agent.recv().expect("Expected SpawnSessionAck");
    match &ack {
        shared_protocol::ProtocolMessage::SpawnSessionAck { session_id, pid, .. } => {
            assert_eq!(session_id.as_str(), sid);
            eprintln!("✓ Spawn OK: pid={}", pid);
        }
        shared_protocol::ProtocolMessage::SpawnSessionNack { reason, .. } => {
            panic!("Spawn NACK: {}", reason);
        }
        other => panic!("Unexpected: {}", other.kind()),
    }

    // Read terminal output — echo produces one chunk then exits (EOF)
    let mut output = Vec::new();
    let mut attempts = 0;
    loop {
        match agent.recv() {
            Some(shared_protocol::ProtocolMessage::TerminalData { session_id, data, seq }) => {
                eprintln!("TerminalData seq={}: {:?}", seq, String::from_utf8_lossy(&data));
                assert_eq!(session_id, sid);
                output.extend_from_slice(&data);

                agent.send(&shared_protocol::ProtocolMessage::Ack {
                    session_id: sid.into(),
                    seq,
                    bytes_consumed: data.len() as u64,
                });

                // echo should produce one line then exit. Give it a moment
                // then send CloseSession.
                break;
            }
            Some(other) => {
                eprintln!("Got: {}", other.kind());
                attempts += 1;
                if attempts > 10 { break; }
            }
            None => break,
        }
    }

    assert!(!output.is_empty(), "Should have received terminal output");
    let output_str = String::from_utf8_lossy(&output);
    eprintln!("Output: {:?}", output_str);
    assert!(output_str.contains("hello_from_pty"), "Output mismatch");
    eprintln!("✓ Output verified");

    // Close session
    agent.send(&shared_protocol::ProtocolMessage::CloseSession { session_id: sid.into() });

    // The agent will respond with CloseSessionAck or Error (session already ended)
    match agent.recv() {
        Some(msg) => eprintln!("Close response: {}", msg.kind()),
        None => eprintln!("No close response (agent may have already cleaned up)"),
    }
}

#[test]
fn test_ping_pong() {
    let mut agent = AgentProcess::start();
    agent.handshake();

    agent.send(&shared_protocol::ProtocolMessage::Ping { nonce: 42 });

    for _ in 0..10 {
        match agent.recv() {
            Some(shared_protocol::ProtocolMessage::Pong { nonce }) => {
                assert_eq!(nonce, 42);
                eprintln!("✓ Ping/pong OK");
                return;
            }
            Some(other) => eprintln!("Unexpected: {}", other.kind()),
            None => break,
        }
    }
    panic!("Never got Pong");
}

#[test]
fn test_error_on_unknown_session() {
    let mut agent = AgentProcess::start();
    agent.handshake();

    agent.send(&shared_protocol::ProtocolMessage::CloseSession {
        session_id: "nonexistent".into(),
    });

    match agent.recv() {
        Some(shared_protocol::ProtocolMessage::Error { code, .. }) => {
            assert_eq!(code, shared_protocol::types::ErrorCode::SessionNotFound);
            eprintln!("✓ Correct error for unknown session");
        }
        other => panic!("Expected Error, got: {:?}", other.map(|m| m.kind().to_string())),
    }
}
