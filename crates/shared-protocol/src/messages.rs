//! Wire-protocol message types.
//!
//! Each message is serialized as a 2-element MessagePack array:
//! `["variant_tag", <binary: msgpack-encoded payload fields>]`
//!
//! This representation is compact, self-describing (the tag is a human-readable
//! string), and avoids the need for an intermediate value representation.

use serde::{Deserialize, Serialize, Deserializer, Serializer, ser::SerializeTuple};
use crate::types::*;

/// The top-level protocol message enum.
///
/// 24 variants covering handshake, session lifecycle, terminal I/O,
/// flow control, structured events, tool management, and keepalive.
#[derive(Debug, Clone)]
pub enum ProtocolMessage {
    // ── Connection Establishment ──────────────────────────────
    Hello { version: u32, capabilities: Vec<String>, session_id: String },
    HelloAck { version: u32, server_version: String, server_arch: String },
    Error { code: ErrorCode, message: String, session_id: Option<String> },
    Goodbye { reason: Option<String> },
    // ── Session Lifecycle ─────────────────────────────────────
    SpawnSession { session_id: String, tool: ToolKind, args: Vec<String>, env: std::collections::HashMap<String, String>, cwd: Option<String>, terminal_cols: u16, terminal_rows: u16, container: Option<String> },
    SpawnSessionAck { session_id: String, pid: u32, tool_version: Option<String> },
    SpawnSessionNack { session_id: String, reason: String },
    CloseSession { session_id: String },
    CloseSessionAck { session_id: String, exit_code: Option<i32> },
    // ── Terminal I/O ──────────────────────────────────────────
    TerminalData { session_id: String, data: Vec<u8>, seq: u64 },
    TerminalInput { session_id: String, data: Vec<u8> },
    TerminalResize { session_id: String, cols: u16, rows: u16 },
    // ── Flow Control ──────────────────────────────────────────
    Ack { session_id: String, seq: u64, bytes_consumed: u64 },
    Pause { session_id: String, reason: PauseReason },
    Resume { session_id: String },
    // ── Structured Session Events ─────────────────────────────
    SessionEvent { session_id: String, event_type: SessionEventType, data: std::collections::HashMap<String, String>, timestamp: u64 },
    // ── Tool Management ───────────────────────────────────────
    ProbeRequest { tool: ToolKind },
    ProbeResponse { tool: ToolKind, installed: bool, version: Option<String>, path: Option<String>, auth_ok: Option<bool>, details: Option<std::collections::HashMap<String, String>> },
    InstallRequest { tool: ToolKind, version: Option<String> },
    InstallProgress { tool: ToolKind, phase: String, progress: f32, message: String },
    InstallComplete { tool: ToolKind, success: bool, version: Option<String>, error: Option<String> },
    // ── Code Change Management ───────────────────────────────
    CodeChange { session_id: String, change_set_id: String, change_id: String, file_path: String, old_content: Option<String>, new_content: Option<String>, diff: String, seq: u64 },
    CodeChangeBatch { session_id: String, change_set_id: String, description: String, status: String, file_count: u32 },
    ApplyChange { session_id: String, file_path: String, content: String },
    // ── Agent Stream Events ──────────────────────────────────
    AgentEvent { session_id: String, kind: AgentEventKind, text: String, code: Option<String>, label: Option<String>, status: Option<String>, seq: u64 },
    // ── Approval Flow ────────────────────────────────────────
    ApprovalRequest { session_id: String, request_id: String, title: String, scope: String, command: String, cwd: Option<String> },
    ApprovalResponse { session_id: String, request_id: String, decision: ApprovalDecision },
    // ── Keepalive ─────────────────────────────────────────────
    Ping { nonce: u64 },
    Pong { nonce: u64 },
}

// ── Tag constants ─────────────────────────────────────────

impl ProtocolMessage {
    /// Returns the wire tag for this variant.
    fn tag(&self) -> &'static str {
        match self {
            Self::Hello { .. } => "hello",
            Self::HelloAck { .. } => "hello_ack",
            Self::Error { .. } => "error",
            Self::Goodbye { .. } => "goodbye",
            Self::SpawnSession { .. } => "spawn_session",
            Self::SpawnSessionAck { .. } => "spawn_session_ack",
            Self::SpawnSessionNack { .. } => "spawn_session_nack",
            Self::CloseSession { .. } => "close_session",
            Self::CloseSessionAck { .. } => "close_session_ack",
            Self::TerminalData { .. } => "terminal_data",
            Self::TerminalInput { .. } => "terminal_input",
            Self::TerminalResize { .. } => "terminal_resize",
            Self::Ack { .. } => "ack",
            Self::Pause { .. } => "pause",
            Self::Resume { .. } => "resume",
            Self::SessionEvent { .. } => "session_event",
            Self::ProbeRequest { .. } => "probe_request",
            Self::ProbeResponse { .. } => "probe_response",
            Self::InstallRequest { .. } => "install_request",
            Self::InstallProgress { .. } => "install_progress",
            Self::InstallComplete { .. } => "install_complete",
            Self::CodeChange { .. } => "code_change",
            Self::CodeChangeBatch { .. } => "code_change_batch",
            Self::ApplyChange { .. } => "apply_change",
            Self::AgentEvent { .. } => "agent_event",
            Self::ApprovalRequest { .. } => "approval_request",
            Self::ApprovalResponse { .. } => "approval_response",
            Self::Ping { .. } => "ping",
            Self::Pong { .. } => "pong",
        }
    }

    /// Convenience: extract session_id if this message carries one.
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::Error { session_id, .. } => session_id.as_deref(),
            Self::SpawnSession { session_id, .. }
            | Self::SpawnSessionAck { session_id, .. }
            | Self::SpawnSessionNack { session_id, .. }
            | Self::CloseSession { session_id }
            | Self::CloseSessionAck { session_id, .. }
            | Self::TerminalData { session_id, .. }
            | Self::TerminalInput { session_id, .. }
            | Self::TerminalResize { session_id, .. }
            | Self::Ack { session_id, .. }
            | Self::Pause { session_id, .. }
            | Self::Resume { session_id }
            | Self::SessionEvent { session_id, .. }
            | Self::CodeChange { session_id, .. }
            | Self::CodeChangeBatch { session_id, .. }
            | Self::ApplyChange { session_id, .. }
            | Self::AgentEvent { session_id, .. }
            | Self::ApprovalRequest { session_id, .. }
            | Self::ApprovalResponse { session_id, .. } => Some(session_id),
            _ => None,
        }
    }

    /// Short label for logging / metrics.
    pub fn kind(&self) -> &'static str {
        self.tag()
    }
}

// ── Payload structs ───────────────────────────────────────
// One per variant, derives Serialize/Deserialize for the wire format.

macro_rules! payload_struct {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Serialize, Deserialize)]
        struct $name { $( $field: $ty ),* }
    };
}

payload_struct!(HelloPayload { version: u32, capabilities: Vec<String>, session_id: String });
payload_struct!(HelloAckPayload { version: u32, server_version: String, server_arch: String });
payload_struct!(ErrorPayload { code: ErrorCode, message: String, session_id: Option<String> });
payload_struct!(GoodbyePayload { reason: Option<String> });
payload_struct!(SpawnSessionPayload { session_id: String, tool: ToolKind, args: Vec<String>, env: std::collections::HashMap<String, String>, cwd: Option<String>, terminal_cols: u16, terminal_rows: u16, container: Option<String> });
payload_struct!(SpawnSessionAckPayload { session_id: String, pid: u32, tool_version: Option<String> });
payload_struct!(SpawnSessionNackPayload { session_id: String, reason: String });
payload_struct!(CloseSessionPayload { session_id: String });
payload_struct!(CloseSessionAckPayload { session_id: String, exit_code: Option<i32> });
payload_struct!(TerminalDataPayload { session_id: String, data: Vec<u8>, seq: u64 });
payload_struct!(TerminalInputPayload { session_id: String, data: Vec<u8> });
payload_struct!(TerminalResizePayload { session_id: String, cols: u16, rows: u16 });
payload_struct!(AckPayload { session_id: String, seq: u64, bytes_consumed: u64 });
payload_struct!(PausePayload { session_id: String, reason: PauseReason });
payload_struct!(ResumePayload { session_id: String });
payload_struct!(SessionEventPayload { session_id: String, event_type: SessionEventType, data: std::collections::HashMap<String, String>, timestamp: u64 });
payload_struct!(ProbeRequestPayload { tool: ToolKind });
payload_struct!(ProbeResponsePayload { tool: ToolKind, installed: bool, version: Option<String>, path: Option<String>, auth_ok: Option<bool>, details: Option<std::collections::HashMap<String, String>> });
payload_struct!(InstallRequestPayload { tool: ToolKind, version: Option<String> });
payload_struct!(InstallProgressPayload { tool: ToolKind, phase: String, progress: f32, message: String });
payload_struct!(InstallCompletePayload { tool: ToolKind, success: bool, version: Option<String>, error: Option<String> });
payload_struct!(CodeChangePayload { session_id: String, change_set_id: String, change_id: String, file_path: String, old_content: Option<String>, new_content: Option<String>, diff: String, seq: u64 });
payload_struct!(CodeChangeBatchPayload { session_id: String, change_set_id: String, description: String, status: String, file_count: u32 });
payload_struct!(ApplyChangePayload { session_id: String, file_path: String, content: String });
payload_struct!(AgentEventPayload { session_id: String, kind: AgentEventKind, text: String, code: Option<String>, label: Option<String>, status: Option<String>, seq: u64 });
payload_struct!(ApprovalRequestPayload { session_id: String, request_id: String, title: String, scope: String, command: String, cwd: Option<String> });
payload_struct!(ApprovalResponsePayload { session_id: String, request_id: String, decision: ApprovalDecision });
payload_struct!(PingPayload { nonce: u64 });
payload_struct!(PongPayload { nonce: u64 });

// ── Serialize implementation ──────────────────────────────
// Converts each variant to (tag, payload_struct), serializes payload
// to msgpack bytes, then writes as a 2-element tuple: [tag, bytes].

impl Serialize for ProtocolMessage {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::Error;

        // Serialize the payload to msgpack bytes
        let payload_bytes = self.serialize_payload()
            .map_err(|e| Error::custom(format!("payload encode: {}", e)))?;

        // Write as 2-element tuple: [tag_string, binary_blob]
        let mut tup = serializer.serialize_tuple(2)?;
        tup.serialize_element(self.tag())?;
        tup.serialize_element(&serde_bytes::ByteBuf::from(payload_bytes))?;
        tup.end()
    }
}

impl ProtocolMessage {
    /// Serialize only the payload (not the tag) to msgpack bytes.
    fn serialize_payload(&self) -> Result<Vec<u8>, rmp_serde::encode::Error> {
        match self {
            Self::Hello { version, capabilities, session_id } =>
                rmp_serde::to_vec(&HelloPayload { version: *version, capabilities: capabilities.clone(), session_id: session_id.clone() }),
            Self::HelloAck { version, server_version, server_arch } =>
                rmp_serde::to_vec(&HelloAckPayload { version: *version, server_version: server_version.clone(), server_arch: server_arch.clone() }),
            Self::Error { code, message, session_id } =>
                rmp_serde::to_vec(&ErrorPayload { code: code.clone(), message: message.clone(), session_id: session_id.clone() }),
            Self::Goodbye { reason } =>
                rmp_serde::to_vec(&GoodbyePayload { reason: reason.clone() }),
            Self::SpawnSession { session_id, tool, args, env, cwd, terminal_cols, terminal_rows, container } =>
                rmp_serde::to_vec(&SpawnSessionPayload { session_id: session_id.clone(), tool: tool.clone(), args: args.clone(), env: env.clone(), cwd: cwd.clone(), terminal_cols: *terminal_cols, terminal_rows: *terminal_rows, container: container.clone() }),
            Self::SpawnSessionAck { session_id, pid, tool_version } =>
                rmp_serde::to_vec(&SpawnSessionAckPayload { session_id: session_id.clone(), pid: *pid, tool_version: tool_version.clone() }),
            Self::SpawnSessionNack { session_id, reason } =>
                rmp_serde::to_vec(&SpawnSessionNackPayload { session_id: session_id.clone(), reason: reason.clone() }),
            Self::CloseSession { session_id } =>
                rmp_serde::to_vec(&CloseSessionPayload { session_id: session_id.clone() }),
            Self::CloseSessionAck { session_id, exit_code } =>
                rmp_serde::to_vec(&CloseSessionAckPayload { session_id: session_id.clone(), exit_code: *exit_code }),
            Self::TerminalData { session_id, data, seq } =>
                rmp_serde::to_vec(&TerminalDataPayload { session_id: session_id.clone(), data: data.clone(), seq: *seq }),
            Self::TerminalInput { session_id, data } =>
                rmp_serde::to_vec(&TerminalInputPayload { session_id: session_id.clone(), data: data.clone() }),
            Self::TerminalResize { session_id, cols, rows } =>
                rmp_serde::to_vec(&TerminalResizePayload { session_id: session_id.clone(), cols: *cols, rows: *rows }),
            Self::Ack { session_id, seq, bytes_consumed } =>
                rmp_serde::to_vec(&AckPayload { session_id: session_id.clone(), seq: *seq, bytes_consumed: *bytes_consumed }),
            Self::Pause { session_id, reason } =>
                rmp_serde::to_vec(&PausePayload { session_id: session_id.clone(), reason: reason.clone() }),
            Self::Resume { session_id } =>
                rmp_serde::to_vec(&ResumePayload { session_id: session_id.clone() }),
            Self::SessionEvent { session_id, event_type, data, timestamp } =>
                rmp_serde::to_vec(&SessionEventPayload { session_id: session_id.clone(), event_type: event_type.clone(), data: data.clone(), timestamp: *timestamp }),
            Self::ProbeRequest { tool } =>
                rmp_serde::to_vec(&ProbeRequestPayload { tool: tool.clone() }),
            Self::ProbeResponse { tool, installed, version, path, auth_ok, details } =>
                rmp_serde::to_vec(&ProbeResponsePayload { tool: tool.clone(), installed: *installed, version: version.clone(), path: path.clone(), auth_ok: *auth_ok, details: details.clone() }),
            Self::InstallRequest { tool, version } =>
                rmp_serde::to_vec(&InstallRequestPayload { tool: tool.clone(), version: version.clone() }),
            Self::InstallProgress { tool, phase, progress, message } =>
                rmp_serde::to_vec(&InstallProgressPayload { tool: tool.clone(), phase: phase.clone(), progress: *progress, message: message.clone() }),
            Self::InstallComplete { tool, success, version, error } =>
                rmp_serde::to_vec(&InstallCompletePayload { tool: tool.clone(), success: *success, version: version.clone(), error: error.clone() }),
            Self::CodeChange { session_id, change_set_id, change_id, file_path, old_content, new_content, diff, seq } =>
                rmp_serde::to_vec(&CodeChangePayload { session_id: session_id.clone(), change_set_id: change_set_id.clone(), change_id: change_id.clone(), file_path: file_path.clone(), old_content: old_content.clone(), new_content: new_content.clone(), diff: diff.clone(), seq: *seq }),
            Self::CodeChangeBatch { session_id, change_set_id, description, status, file_count } =>
                rmp_serde::to_vec(&CodeChangeBatchPayload { session_id: session_id.clone(), change_set_id: change_set_id.clone(), description: description.clone(), status: status.clone(), file_count: *file_count }),
            Self::ApplyChange { session_id, file_path, content } =>
                rmp_serde::to_vec(&ApplyChangePayload { session_id: session_id.clone(), file_path: file_path.clone(), content: content.clone() }),
            Self::AgentEvent { session_id, kind, text, code, label, status, seq } =>
                rmp_serde::to_vec(&AgentEventPayload { session_id: session_id.clone(), kind: kind.clone(), text: text.clone(), code: code.clone(), label: label.clone(), status: status.clone(), seq: *seq }),
            Self::ApprovalRequest { session_id, request_id, title, scope, command, cwd } =>
                rmp_serde::to_vec(&ApprovalRequestPayload { session_id: session_id.clone(), request_id: request_id.clone(), title: title.clone(), scope: scope.clone(), command: command.clone(), cwd: cwd.clone() }),
            Self::ApprovalResponse { session_id, request_id, decision } =>
                rmp_serde::to_vec(&ApprovalResponsePayload { session_id: session_id.clone(), request_id: request_id.clone(), decision: decision.clone() }),
            Self::Ping { nonce } =>
                rmp_serde::to_vec(&PingPayload { nonce: *nonce }),
            Self::Pong { nonce } =>
                rmp_serde::to_vec(&PongPayload { nonce: *nonce }),
        }
    }

    fn deserialize_payload(tag: &str, bytes: &[u8]) -> Result<Self, String> {
        Ok(match tag {
            "hello" => { let p: HelloPayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::Hello { version: p.version, capabilities: p.capabilities, session_id: p.session_id } }
            "hello_ack" => { let p: HelloAckPayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::HelloAck { version: p.version, server_version: p.server_version, server_arch: p.server_arch } }
            "error" => { let p: ErrorPayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::Error { code: p.code, message: p.message, session_id: p.session_id } }
            "goodbye" => { let p: GoodbyePayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::Goodbye { reason: p.reason } }
            "spawn_session" => { let p: SpawnSessionPayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::SpawnSession { session_id: p.session_id, tool: p.tool, args: p.args, env: p.env, cwd: p.cwd, terminal_cols: p.terminal_cols, terminal_rows: p.terminal_rows, container: p.container } }
            "spawn_session_ack" => { let p: SpawnSessionAckPayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::SpawnSessionAck { session_id: p.session_id, pid: p.pid, tool_version: p.tool_version } }
            "spawn_session_nack" => { let p: SpawnSessionNackPayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::SpawnSessionNack { session_id: p.session_id, reason: p.reason } }
            "close_session" => { let p: CloseSessionPayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::CloseSession { session_id: p.session_id } }
            "close_session_ack" => { let p: CloseSessionAckPayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::CloseSessionAck { session_id: p.session_id, exit_code: p.exit_code } }
            "terminal_data" => { let p: TerminalDataPayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::TerminalData { session_id: p.session_id, data: p.data, seq: p.seq } }
            "terminal_input" => { let p: TerminalInputPayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::TerminalInput { session_id: p.session_id, data: p.data } }
            "terminal_resize" => { let p: TerminalResizePayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::TerminalResize { session_id: p.session_id, cols: p.cols, rows: p.rows } }
            "ack" => { let p: AckPayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::Ack { session_id: p.session_id, seq: p.seq, bytes_consumed: p.bytes_consumed } }
            "pause" => { let p: PausePayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::Pause { session_id: p.session_id, reason: p.reason } }
            "resume" => { let p: ResumePayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::Resume { session_id: p.session_id } }
            "session_event" => { let p: SessionEventPayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::SessionEvent { session_id: p.session_id, event_type: p.event_type, data: p.data, timestamp: p.timestamp } }
            "probe_request" => { let p: ProbeRequestPayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::ProbeRequest { tool: p.tool } }
            "probe_response" => { let p: ProbeResponsePayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::ProbeResponse { tool: p.tool, installed: p.installed, version: p.version, path: p.path, auth_ok: p.auth_ok, details: p.details } }
            "install_request" => { let p: InstallRequestPayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::InstallRequest { tool: p.tool, version: p.version } }
            "install_progress" => { let p: InstallProgressPayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::InstallProgress { tool: p.tool, phase: p.phase, progress: p.progress, message: p.message } }
            "install_complete" => { let p: InstallCompletePayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::InstallComplete { tool: p.tool, success: p.success, version: p.version, error: p.error } }
            "code_change" => { let p: CodeChangePayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::CodeChange { session_id: p.session_id, change_set_id: p.change_set_id, change_id: p.change_id, file_path: p.file_path, old_content: p.old_content, new_content: p.new_content, diff: p.diff, seq: p.seq } }
            "code_change_batch" => { let p: CodeChangeBatchPayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::CodeChangeBatch { session_id: p.session_id, change_set_id: p.change_set_id, description: p.description, status: p.status, file_count: p.file_count } }
            "apply_change" => { let p: ApplyChangePayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::ApplyChange { session_id: p.session_id, file_path: p.file_path, content: p.content } }
            "agent_event" => { let p: AgentEventPayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::AgentEvent { session_id: p.session_id, kind: p.kind, text: p.text, code: p.code, label: p.label, status: p.status, seq: p.seq } }
            "approval_request" => { let p: ApprovalRequestPayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::ApprovalRequest { session_id: p.session_id, request_id: p.request_id, title: p.title, scope: p.scope, command: p.command, cwd: p.cwd } }
            "approval_response" => { let p: ApprovalResponsePayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::ApprovalResponse { session_id: p.session_id, request_id: p.request_id, decision: p.decision } }
            "ping" => { let p: PingPayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::Ping { nonce: p.nonce } }
            "pong" => { let p: PongPayload = rmp_serde::from_slice(bytes).map_err(|e| e.to_string())?; Self::Pong { nonce: p.nonce } }
            _ => return Err(format!("unknown tag: {}", tag)),
        })
    }
}

// ── Deserialize implementation ────────────────────────────

impl<'de> Deserialize<'de> for ProtocolMessage {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let (tag, blob): (String, serde_bytes::ByteBuf) = Deserialize::deserialize(deserializer)?;
        Self::deserialize_payload(&tag, &blob)
            .map_err(|e| Error::custom(e))
    }
}

// ── Tests ─────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id_extraction() {
        let msg = ProtocolMessage::SpawnSession {
            session_id: "abc-123".into(),
            tool: ToolKind::Claude, args: vec![], env: Default::default(),
            cwd: None, terminal_cols: 80, terminal_rows: 24, container: None,
        };
        assert_eq!(msg.session_id(), Some("abc-123"));
    }

    #[test]
    fn test_message_without_session_id() {
        let msg = ProtocolMessage::Hello { version: 1, capabilities: vec![], session_id: "host-1".into() };
        assert_eq!(msg.session_id(), None);
    }

    #[test]
    fn test_message_kind() {
        assert_eq!(ProtocolMessage::Ping { nonce: 42 }.kind(), "ping");
    }

    #[test]
    fn test_serde_roundtrip() {
        let msg = ProtocolMessage::SpawnSessionAck {
            session_id: "s1".into(), pid: 12345, tool_version: Some("0.8.0".into()),
        };
        let buf = rmp_serde::to_vec(&msg).unwrap();
        let recovered: ProtocolMessage = rmp_serde::from_slice(&buf).unwrap();
        match recovered {
            ProtocolMessage::SpawnSessionAck { session_id, pid, tool_version } => {
                assert_eq!(session_id, "s1");
                assert_eq!(pid, 12345);
                assert_eq!(tool_version.unwrap(), "0.8.0");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_terminal_data_roundtrip() {
        let msg = ProtocolMessage::TerminalData {
            session_id: "s2".into(), data: vec![0x1b, 0x5b, 0x33, 0x31, 0x6d], seq: 7,
        };
        let buf = rmp_serde::to_vec(&msg).unwrap();
        let recovered: ProtocolMessage = rmp_serde::from_slice(&buf).unwrap();
        match recovered {
            ProtocolMessage::TerminalData { session_id, data, seq } => {
                assert_eq!(session_id, "s2");
                assert_eq!(data, vec![0x1b, 0x5b, 0x33, 0x31, 0x6d]);
                assert_eq!(seq, 7);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_agent_event_roundtrip() {
        let msg = ProtocolMessage::AgentEvent {
            session_id: "s3".into(),
            kind: AgentEventKind::Action,
            text: "writing rollback logic".into(),
            code: Some("None => install_cli(host, \"npm\").await?;".into()),
            label: Some("写入 main.rs L11-L12".into()),
            status: None,
            seq: 12,
        };
        let buf = rmp_serde::to_vec(&msg).unwrap();
        let recovered: ProtocolMessage = rmp_serde::from_slice(&buf).unwrap();
        match recovered {
            ProtocolMessage::AgentEvent { session_id, kind, text, code, label, status, seq } => {
                assert_eq!(session_id, "s3");
                assert_eq!(kind, AgentEventKind::Action);
                assert_eq!(text, "writing rollback logic");
                assert_eq!(code.unwrap(), "None => install_cli(host, \"npm\").await?;");
                assert_eq!(label.unwrap(), "写入 main.rs L11-L12");
                assert!(status.is_none());
                assert_eq!(seq, 12);
            }
            _ => panic!("wrong variant"),
        }
        assert_eq!(msg.session_id(), Some("s3"));
        assert_eq!(msg.kind(), "agent_event");
    }

    #[test]
    fn test_approval_request_roundtrip() {
        let msg = ProtocolMessage::ApprovalRequest {
            session_id: "s4".into(),
            request_id: "req-1".into(),
            title: "Agent 申请执行".into(),
            scope: "servers.cargo".into(),
            command: "cargo build --release".into(),
            cwd: Some("/myproject".into()),
        };
        let buf = rmp_serde::to_vec(&msg).unwrap();
        let recovered: ProtocolMessage = rmp_serde::from_slice(&buf).unwrap();
        match recovered {
            ProtocolMessage::ApprovalRequest { session_id, request_id, title, scope, command, cwd } => {
                assert_eq!(session_id, "s4");
                assert_eq!(request_id, "req-1");
                assert_eq!(title, "Agent 申请执行");
                assert_eq!(scope, "servers.cargo");
                assert_eq!(command, "cargo build --release");
                assert_eq!(cwd.unwrap(), "/myproject");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_approval_response_roundtrip() {
        let msg = ProtocolMessage::ApprovalResponse {
            session_id: "s5".into(),
            request_id: "req-2".into(),
            decision: ApprovalDecision::AllowAll,
        };
        let buf = rmp_serde::to_vec(&msg).unwrap();
        let recovered: ProtocolMessage = rmp_serde::from_slice(&buf).unwrap();
        match recovered {
            ProtocolMessage::ApprovalResponse { session_id, request_id, decision } => {
                assert_eq!(session_id, "s5");
                assert_eq!(request_id, "req-2");
                assert_eq!(decision, ApprovalDecision::AllowAll);
            }
            _ => panic!("wrong variant"),
        }
    }
}
