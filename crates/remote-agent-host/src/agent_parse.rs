//! Agent stream parser — converts an agent CLI's structured stream-json
//! output (newline-delimited JSON) into protocol [`AgentEvent`] /
//! [`ApprovalRequest`] messages.
//!
//! The parser is intentionally pure and incremental: feed it one line at a
//! time via [`AgentStreamParser::push_line`], and it returns any protocol
//! messages derived from that line. Non-JSON lines (raw terminal noise) and
//! unrecognized shapes yield no messages, so it is safe to run over a PTY
//! byte stream where only some lines are well-formed JSON.
//!
//! Supported (Claude Code `--output-format stream-json`) shapes:
//! - assistant `thinking` block  → `AgentEvent { kind: Thought }`
//! - assistant `text` block      → `AgentEvent { kind: Thought }`
//! - assistant `tool_use` block  → `AgentEvent { kind: Action }`
//! - user `tool_result` block    → `AgentEvent { kind: Observation }`
//! - `control_request` / `can_use_tool` permission prompt → `ApprovalRequest`

use serde_json::Value;
use shared_protocol::messages::ProtocolMessage;
use shared_protocol::types::AgentEventKind;

/// Incremental, stateful parser for one session's agent stream.
pub struct AgentStreamParser {
    session_id: String,
    seq: u64,
}

impl AgentStreamParser {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self { session_id: session_id.into(), seq: 0 }
    }

    fn next_seq(&mut self) -> u64 {
        let s = self.seq;
        self.seq += 1;
        s
    }

    fn agent_event(
        &mut self,
        kind: AgentEventKind,
        text: String,
        code: Option<String>,
        label: Option<String>,
        status: Option<String>,
    ) -> ProtocolMessage {
        ProtocolMessage::AgentEvent {
            session_id: self.session_id.clone(),
            kind,
            text,
            code,
            label,
            status,
            seq: self.next_seq(),
        }
    }

    /// Feed one line of stream output. Returns derived protocol messages.
    pub fn push_line(&mut self, line: &str) -> Vec<ProtocolMessage> {
        let line = line.trim();
        if line.is_empty() || !line.starts_with('{') {
            return Vec::new();
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        self.parse_value(&value)
    }

    fn parse_value(&mut self, v: &Value) -> Vec<ProtocolMessage> {
        let mut out = Vec::new();
        let ty = v.get("type").and_then(Value::as_str).unwrap_or("");

        match ty {
            "assistant" => {
                for block in message_content(v) {
                    self.parse_assistant_block(block, &mut out);
                }
            }
            "user" => {
                for block in message_content(v) {
                    self.parse_user_block(block, &mut out);
                }
            }
            // Permission prompt shapes: a top-level control_request, or a
            // can_use_tool subtype carried in `request`.
            "control_request" | "can_use_tool" | "permission_request" => {
                if let Some(req) = self.parse_permission(v) {
                    out.push(req);
                }
            }
            _ => {}
        }
        out
    }

    fn parse_assistant_block(&mut self, block: &Value, out: &mut Vec<ProtocolMessage>) {
        let bty = block.get("type").and_then(Value::as_str).unwrap_or("");
        match bty {
            "thinking" => {
                let text = block
                    .get("thinking")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if !text.is_empty() {
                    out.push(self.agent_event(AgentEventKind::Thought, text, None, None, None));
                }
            }
            "text" => {
                let text = block.get("text").and_then(Value::as_str).unwrap_or("").to_string();
                if !text.is_empty() {
                    out.push(self.agent_event(AgentEventKind::Thought, text, None, None, None));
                }
            }
            "tool_use" => {
                let tool = block.get("name").and_then(Value::as_str).unwrap_or("tool");
                let input = block.get("input");
                let (label, code) = describe_tool_use(tool, input);
                out.push(self.agent_event(
                    AgentEventKind::Action,
                    String::new(),
                    code,
                    Some(label),
                    Some("running".to_string()),
                ));
            }
            _ => {}
        }
    }

    fn parse_user_block(&mut self, block: &Value, out: &mut Vec<ProtocolMessage>) {
        if block.get("type").and_then(Value::as_str) == Some("tool_result") {
            let text = extract_tool_result_text(block);
            let is_error = block.get("is_error").and_then(Value::as_bool).unwrap_or(false);
            out.push(self.agent_event(
                AgentEventKind::Observation,
                text,
                None,
                None,
                Some(if is_error { "error".into() } else { "done".into() }),
            ));
        }
    }

    fn parse_permission(&mut self, v: &Value) -> Option<ProtocolMessage> {
        // The permission details may be at the top level or nested in `request`.
        let req = v.get("request").unwrap_or(v);
        let tool = req
            .get("tool_name")
            .or_else(|| req.get("tool"))
            .and_then(Value::as_str)
            .unwrap_or("tool");
        let input = req.get("input");
        let (label, command) = describe_tool_use(tool, input);
        let request_id = v
            .get("request_id")
            .or_else(|| req.get("request_id"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let cwd = input
            .and_then(|i| i.get("cwd"))
            .and_then(Value::as_str)
            .map(str::to_string);

        Some(ProtocolMessage::ApprovalRequest {
            session_id: self.session_id.clone(),
            request_id,
            title: format!("Agent 申请执行: {}", tool),
            scope: label,
            command: command.unwrap_or_else(|| tool.to_string()),
            cwd,
        })
    }
}

/// Returns the `message.content` array of a stream object, or an empty slice.
fn message_content(v: &Value) -> &[Value] {
    v.get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Build a (label, code) pair describing a tool_use / permission input.
fn describe_tool_use(tool: &str, input: Option<&Value>) -> (String, Option<String>) {
    let Some(input) = input else {
        return (tool.to_string(), None);
    };
    match tool {
        "Bash" => {
            let cmd = input.get("command").and_then(Value::as_str).unwrap_or("");
            (format!("bash: {}", first_words(cmd, 6)), Some(cmd.to_string()))
        }
        "Edit" | "Write" | "edit_file" | "write_file" => {
            let path = input
                .get("file_path")
                .or_else(|| input.get("path"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let snippet = input
                .get("new_string")
                .or_else(|| input.get("content"))
                .and_then(Value::as_str)
                .map(|s| s.to_string());
            (format!("{} {}", tool, path), snippet)
        }
        "Read" | "read_file" => {
            // Surface the path in the label so the desktop can track which
            // files are in the agent's working context ("Show Active Files").
            let path = input
                .get("file_path")
                .or_else(|| input.get("path"))
                .and_then(Value::as_str)
                .unwrap_or("");
            (format!("Read {}", path), None)
        }
        _ => {
            // Generic: serialize the input compactly as the code body.
            let code = serde_json::to_string(input).ok();
            (tool.to_string(), code)
        }
    }
}

/// tool_result content can be a string or an array of {type:text, text}.
fn extract_tool_result_text(block: &Value) -> String {
    match block.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|it| it.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn first_words(s: &str, n: usize) -> String {
    s.split_whitespace().take(n).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(msgs: Vec<ProtocolMessage>) -> ProtocolMessage {
        assert_eq!(msgs.len(), 1, "expected exactly one message, got {}", msgs.len());
        msgs.into_iter().next().unwrap()
    }

    #[test]
    fn ignores_non_json_and_blank() {
        let mut p = AgentStreamParser::new("s1");
        assert!(p.push_line("").is_empty());
        assert!(p.push_line("warning: unused import").is_empty());
        assert!(p.push_line("[detector] probing host...").is_empty());
        assert!(p.push_line("{ not valid json").is_empty());
    }

    #[test]
    fn parses_thinking_to_thought() {
        let mut p = AgentStreamParser::new("s1");
        let line = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"need to check the CLI first"}]}}"#;
        match one(p.push_line(line)) {
            ProtocolMessage::AgentEvent { kind, text, seq, .. } => {
                assert_eq!(kind, AgentEventKind::Thought);
                assert_eq!(text, "need to check the CLI first");
                assert_eq!(seq, 0);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_bash_tool_use_to_action() {
        let mut p = AgentStreamParser::new("s1");
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"cargo build --release"}}]}}"#;
        match one(p.push_line(line)) {
            ProtocolMessage::AgentEvent { kind, code, label, status, .. } => {
                assert_eq!(kind, AgentEventKind::Action);
                assert_eq!(code.unwrap(), "cargo build --release");
                assert_eq!(label.unwrap(), "bash: cargo build --release");
                assert_eq!(status.unwrap(), "running");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_edit_tool_use_with_path() {
        let mut p = AgentStreamParser::new("s1");
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Edit","input":{"file_path":"src/main.rs","new_string":"None => install_cli()"}}]}}"#;
        match one(p.push_line(line)) {
            ProtocolMessage::AgentEvent { kind, code, label, .. } => {
                assert_eq!(kind, AgentEventKind::Action);
                assert_eq!(label.unwrap(), "Edit src/main.rs");
                assert_eq!(code.unwrap(), "None => install_cli()");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_tool_result_to_observation() {
        let mut p = AgentStreamParser::new("s1");
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"Compiling shared-protocol v0.1.0"}]}}"#;
        match one(p.push_line(line)) {
            ProtocolMessage::AgentEvent { kind, text, status, .. } => {
                assert_eq!(kind, AgentEventKind::Observation);
                assert_eq!(text, "Compiling shared-protocol v0.1.0");
                assert_eq!(status.unwrap(), "done");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn tool_result_array_content_and_error_flag() {
        let mut p = AgentStreamParser::new("s1");
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":true,"content":[{"type":"text","text":"boom"}]}]}}"#;
        match one(p.push_line(line)) {
            ProtocolMessage::AgentEvent { text, status, .. } => {
                assert_eq!(text, "boom");
                assert_eq!(status.unwrap(), "error");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_permission_request_to_approval() {
        let mut p = AgentStreamParser::new("s7");
        let line = r#"{"type":"control_request","request_id":"req-9","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"rm -rf build","cwd":"/proj"}}}"#;
        match one(p.push_line(line)) {
            ProtocolMessage::ApprovalRequest { session_id, request_id, command, cwd, scope, .. } => {
                assert_eq!(session_id, "s7");
                assert_eq!(request_id, "req-9");
                assert_eq!(command, "rm -rf build");
                assert_eq!(cwd.unwrap(), "/proj");
                assert_eq!(scope, "bash: rm -rf build");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn seq_increments_across_lines() {
        let mut p = AgentStreamParser::new("s1");
        let a = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"one"}]}}"#;
        let b = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"two"}]}}"#;
        let seq_a = match one(p.push_line(a)) {
            ProtocolMessage::AgentEvent { seq, .. } => seq,
            _ => panic!(),
        };
        let seq_b = match one(p.push_line(b)) {
            ProtocolMessage::AgentEvent { seq, .. } => seq,
            _ => panic!(),
        };
        assert_eq!(seq_a, 0);
        assert_eq!(seq_b, 1);
    }

    #[test]
    fn multiple_blocks_in_one_message() {
        let mut p = AgentStreamParser::new("s1");
        let line = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"plan"},{"type":"tool_use","name":"Bash","input":{"command":"ls"}}]}}"#;
        let msgs = p.push_line(line);
        assert_eq!(msgs.len(), 2);
    }
}
