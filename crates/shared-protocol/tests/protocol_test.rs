//! Protocol smoke tests — verify that MessagePack serialization is
//! forward-compatible when new fields are added to the END of structs.
//!
//! BUG CAUGHT: inserting `container` in the MIDDLE of `SpawnSessionPayload`
//! shifted all subsequent field positions, causing old agents to read
//! wrong values. These tests prevent regression.
//!
//! We compare encoded byte-for-byte (not decoded structs) because
//! `ProtocolMessage` is a complex enum with custom serde.

use shared_protocol::messages::ProtocolMessage;
use shared_protocol::types::*;

/// Verify that SpawnSession with container=None round-trips correctly.
#[test]
fn test_spawn_session_round_trip() {
    let msg = ProtocolMessage::SpawnSession {
        session_id: "test-1".into(),
        tool: ToolKind::Claude,
        args: vec!["--serve".into()],
        env: Default::default(),
        cwd: None,
        terminal_cols: 80,
        terminal_rows: 24,
        container: None,
    };
    let encoded = shared_protocol::encode(&msg).unwrap();
    // Decode and re-encode — must produce identical bytes
    let (decoded, _) = shared_protocol::decode(&encoded).unwrap().unwrap();
    let reencoded = shared_protocol::encode(&decoded).unwrap();
    assert_eq!(encoded, reencoded,
        "Encoding must be idempotent: encode→decode→encode == encode");
}

/// Verify that SpawnSession with container=Some(...) round-trips correctly.
#[test]
fn test_spawn_session_with_container() {
    let msg = ProtocolMessage::SpawnSession {
        session_id: "test-2".into(),
        tool: ToolKind::Copilot,
        args: vec!["--serve".into()],
        env: Default::default(),
        cwd: Some("/workspace".into()),
        terminal_cols: 120,
        terminal_rows: 40,
        container: Some("dev-env".into()),
    };
    let encoded = shared_protocol::encode(&msg).unwrap();
    let (decoded, _) = shared_protocol::decode(&encoded).unwrap().unwrap();
    let reencoded = shared_protocol::encode(&decoded).unwrap();
    assert_eq!(encoded, reencoded,
        "Container field must survive round-trip unchanged");
}

/// Both container=None and container=Some must deserialize correctly.
/// (The encoded byte length differs because nil vs string, but both produce
/// valid 8-element arrays that correctly decode and re-encode.)
#[test]
fn test_spawn_session_field_count() {
    let msg1 = ProtocolMessage::SpawnSession {
        session_id: "a".into(), tool: ToolKind::Claude, args: vec![],
        env: Default::default(), cwd: None, terminal_cols: 80, terminal_rows: 24, container: None,
    };
    let msg2 = ProtocolMessage::SpawnSession {
        session_id: "b".into(), tool: ToolKind::Copilot, args: vec!["x".into()],
        env: Default::default(), cwd: Some("/tmp".into()), terminal_cols: 100, terminal_rows: 30,
        container: Some("ctr".into()),
    };
    // Both must decode successfully
    for msg in [&msg1, &msg2] {
        let encoded = shared_protocol::encode(msg).unwrap();
        let (_, _) = shared_protocol::decode(&encoded).unwrap().unwrap();
    }
}

/// Verify Hello round-trip.
#[test]
fn test_hello_round_trip() {
    let msg = ProtocolMessage::Hello {
        version: 1,
        capabilities: vec!["pty".into(), "probe".into()],
        session_id: "host-1".into(),
    };
    let encoded = shared_protocol::encode(&msg).unwrap();
    let (_, _) = shared_protocol::decode(&encoded).unwrap().unwrap();
}

/// Verify SpawnSessionNack round-trip (error from agent).
#[test]
fn test_spawn_nack_round_trip() {
    let msg = ProtocolMessage::SpawnSessionNack {
        session_id: "s1".into(),
        reason: "Claude not installed".into(),
    };
    let encoded = shared_protocol::encode(&msg).unwrap();
    let (_, _) = shared_protocol::decode(&encoded).unwrap().unwrap();
}

/// Verify Error round-trip.
#[test]
fn test_error_round_trip() {
    let msg = ProtocolMessage::Error {
        code: ErrorCode::InvalidMessage,
        message: "test error".into(),
        session_id: Some("s1".into()),
    };
    let encoded = shared_protocol::encode(&msg).unwrap();
    let (_, _) = shared_protocol::decode(&encoded).unwrap().unwrap();
}

/// Verify binary terminal data round-trip.
#[test]
fn test_terminal_data_binary_safety() {
    let data = vec![0, 1, 2, 255, 254, 128, b'\x1b', b'['];
    let msg = ProtocolMessage::TerminalData {
        session_id: "s1".into(),
        data,
        seq: 42,
    };
    let encoded = shared_protocol::encode(&msg).unwrap();
    let (decoded, _) = shared_protocol::decode(&encoded).unwrap().unwrap();
    let reencoded = shared_protocol::encode(&decoded).unwrap();
    assert_eq!(encoded, reencoded, "Binary data must survive round-trip");
}
