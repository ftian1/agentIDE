//! Integration test exercising the full IPC transport flow.
//!
//! Spawns the agent, performs the Hello handshake, creates a session
//! running `cat` (which echoes stdin to stdout), writes input, reads output,
//! and verifies the echo.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

use shared_protocol::{MessageDecoder, ProtocolMessage};
use shared_protocol::types::ToolKind;

fn encode(msg: &ProtocolMessage) -> Vec<u8> {
    shared_protocol::encode_to_vec(msg).unwrap()
}

fn read_msg<R: Read>(r: &mut R, decoder: &mut MessageDecoder, buf: &mut [u8]) -> Option<ProtocolMessage> {
    loop {
        match r.read(buf) {
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
}

#[test]
fn test_cat_session_echo() {
    // Spawn agent
    let bin = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/test-agent");
    let mut child = Command::new(bin)
        .arg("--mode").arg("stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn agent");

    let stdin = child.stdin.as_mut().unwrap();
    let stdout = child.stdout.as_mut().unwrap();
    std::thread::sleep(Duration::from_millis(200));

    let mut decoder = MessageDecoder::new();
    let mut buf = [0u8; 65536];

    // 1. Read Hello
    let hello = read_msg(stdout, &mut decoder, &mut buf).expect("Hello");
    assert!(matches!(hello, ProtocolMessage::Hello { .. }));

    // 2. Send HelloAck
    stdin.write_all(&encode(&ProtocolMessage::HelloAck {
        version: 1, server_version: "test".into(), server_arch: "x86_64".into(),
    })).unwrap();
    stdin.flush().unwrap();

    // 3. Spawn `cat` session
    let sid = "cat-test";
    stdin.write_all(&encode(&ProtocolMessage::SpawnSession {
        session_id: sid.into(),
        tool: ToolKind::Custom("cat".into()),
        args: vec![],
        env: Default::default(),
        cwd: None,
        terminal_cols: 80,
        terminal_rows: 24,
        container: None,
    })).unwrap();
    stdin.flush().unwrap();

    // 4. Read SpawnSessionAck
    let ack = read_msg(stdout, &mut decoder, &mut buf).expect("SpawnAck");
    let pid = match ack {
        ProtocolMessage::SpawnSessionAck { pid, .. } => pid,
        ProtocolMessage::SpawnSessionNack { reason, .. } => panic!("Spawn NACK: {}", reason),
        other => panic!("Expected SpawnAck, got: {}", other.kind()),
    };
    eprintln!("cat session spawned: pid={}", pid);

    // 5. Write "hello from stdin\n" to the cat session
    let test_input = b"hello from stdin\n";
    stdin.write_all(&encode(&ProtocolMessage::TerminalInput {
        session_id: sid.into(),
        data: test_input.to_vec(),
    })).unwrap();
    stdin.flush().unwrap();

    // 6. Read TerminalData — cat echoes the input back
    let mut echoed = Vec::new();
    let mut got_echo = false;
    loop {
        match read_msg(stdout, &mut decoder, &mut buf) {
            Some(ProtocolMessage::TerminalData { session_id, data, seq }) if session_id == sid => {
                eprintln!("TerminalData seq={}: {:?}", seq, String::from_utf8_lossy(&data));
                echoed.extend_from_slice(&data);
                stdin.write_all(&encode(&ProtocolMessage::Ack {
                    session_id: sid.into(), seq, bytes_consumed: data.len() as u64,
                })).unwrap();
                stdin.flush().unwrap();

                // Check if we got the echo (allow \r\n)
                if String::from_utf8_lossy(&echoed).contains("hello from stdin") {
                    got_echo = true;
                    break;
                }
            }
            Some(other) => {
                eprintln!("Other: {}", other.kind());
                // Give a few tries for non-terminal-data messages
            }
            None => break,
        }
    }
    assert!(got_echo, "Should receive echo of input");

    // 7. Send Ctrl-D to close cat's stdin
    stdin.write_all(&encode(&ProtocolMessage::TerminalInput {
        session_id: sid.into(),
        data: vec![0x04], // Ctrl-D
    })).unwrap();
    stdin.flush().unwrap();

    // 8. Close session
    stdin.write_all(&encode(&ProtocolMessage::CloseSession { session_id: sid.into() })).unwrap();
    stdin.flush().unwrap();

    // Give it time, then kill
    std::thread::sleep(Duration::from_millis(100));
    let _ = child.kill();
    let _ = child.wait();

    eprintln!("✓ cat session echo test passed");
}
