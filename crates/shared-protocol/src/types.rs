//! Core types shared across all layers of the IDE.
//!
//! These types define enums and structs for tool kinds, session states,
//! session events, and error codes. They appear in protocol messages.

use serde::{Deserialize, Serialize};

/// Which AI agent CLI tool is being managed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Claude,
    Copilot,
    #[serde(untagged)]
    Custom(String),
}

impl ToolKind {
    /// The default CLI command name for this tool.
    pub fn default_command(&self) -> &str {
        match self {
            ToolKind::Claude => "claude",
            ToolKind::Copilot => "gh",
            ToolKind::Custom(name) => name.as_str(),
        }
    }

    /// Display name shown in UI.
    pub fn display_name(&self) -> &str {
        match self {
            ToolKind::Claude => "Claude",
            ToolKind::Copilot => "GitHub Copilot",
            ToolKind::Custom(name) => name.as_str(),
        }
    }
}

/// Lifecycle state of a managed session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Session is being created (PTY starting up).
    Spawning,
    /// Session is running normally.
    Running,
    /// Session output is paused (flow control or user request).
    Paused(PauseReason),
    /// Session has ended.
    Ended(EndReason),
}

/// Reason the session was paused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PauseReason {
    /// Backpressure: client hasn't acked enough data.
    FlowControl,
    /// User explicitly paused the session.
    UserRequest,
    /// Paused due to an error condition.
    Error(String),
}

/// Reason the session ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    /// User clicked close.
    UserClosed,
    /// Process exited with exit code.
    ProcessExited(i32),
    /// Process or PTY crashed.
    Crashed(String),
    /// SSH connection was lost.
    ConnectionLost,
    /// Session timed out (no activity).
    TimedOut,
}

/// Structured events emitted during a session's lifetime.
/// These enable frontend features like turn tracking, cost monitoring,
/// and subagent detection without parsing raw terminal output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEventType {
    /// A new conversation turn started.
    TurnStart,
    /// A conversation turn ended.
    TurnEnd,
    /// A tool was invoked.
    ToolCall,
    /// A tool returned a result.
    ToolResult,
    /// A sub-agent was spawned.
    SubagentSpawned,
    /// A sub-agent session ended.
    SubagentEnded,
    /// A shell command was executed.
    ShellCommand,
    /// Cost/token update. Data fields: input_tokens, output_tokens, cost_usd.
    CostUpdate,
    /// Token metric snapshot. Data: prompt_tokens, completion_tokens, cache_hit_tokens.
    TokenMetric,
    /// An error occurred.
    Error,
    /// Warning (non-fatal).
    Warning,
    /// Informational event.
    Info,
}

/// Error codes used in the `Error` protocol message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidMessage,
    SessionNotFound,
    SessionAlreadyExists,
    SpawnFailed,
    PtyError,
    TransportError,
    BootstrapFailed,
    ToolNotFound,
    InstallFailed,
    AuthFailed,
    InternalError,
    Timeout,
    UnsupportedArchitecture,
}

/// Session metadata captured at spawn time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    /// Working directory on the remote host.
    pub cwd: Option<String>,
    /// Environment variables set for the session.
    pub env: std::collections::HashMap<String, String>,
    /// Terminal columns at spawn.
    pub terminal_cols: u16,
    /// Terminal rows at spawn.
    pub terminal_rows: u16,
    /// CLI arguments passed.
    pub args: Vec<String>,
}

/// Summary info about a session for the registry listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub tool: ToolKind,
    pub state: SessionState,
    pub pid: u32,
    pub created_at: u64, // unix timestamp millis
    pub turn_count: u64,
}

/// Information about the remote Linux host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteHostInfo {
    pub arch: String,
    pub platform: String,
    pub home_dir: String,
    pub user: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_version: Option<String>,
}

/// Result of probing for a CLI tool on the remote host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub tool: ToolKind,
    pub installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Whether the tool is authenticated (logged in).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_ok: Option<bool>,
    /// Additional details (e.g., plan type, endpoint).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<std::collections::HashMap<String, String>>,
}

/// Cost breakdown for a session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CostBreakdown {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub cost_usd: f64,
}
