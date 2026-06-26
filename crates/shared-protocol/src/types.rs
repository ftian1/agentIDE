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

/// Configuration for a third-party LLM provider, passed from the frontend
/// to the remote proxy so it can route requests by model_id.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub id: String,
    pub kind: String, // "copilot" | "deepseek" | "openai-compatible" | "ollama" | "openrouter" | "groq" | "gemini"
    pub label: String,
    pub base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copilot_token: Option<String>,
    /// Model IDs that belong to this provider (e.g. ["deepseek-chat", "deepseek-reasoner"])
    pub model_ids: Vec<String>,
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

/// Kind of agent reasoning block surfaced from the agent CLI's structured stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentEventKind {
    /// Model reasoning / thinking text.
    Thought,
    /// A tool invocation (edit_file, bash, etc.).
    Action,
    /// A tool result / observation.
    Observation,
}

/// User's decision on an agent permission request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ApprovalDecision {
    /// Allow this single request.
    Allow,
    /// Allow this and all future requests of the same kind.
    AllowAll,
    /// Reject this request.
    Reject,
}

/// How the HTTP tap captured an exchange.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TapMode {
    /// Forward proxy with TLS man-in-the-middle (HTTPS_PROXY + injected CA).
    Mitm,
    /// Reverse proxy via base-URL injection (no TLS interception).
    Reverse,
}

/// A single captured HTTP request/response exchange from the agent CLI.
///
/// Bodies are captured up to a cap (see the tap proxy); `truncated` marks
/// exchanges where either body exceeded it. Sensitive auth headers are
/// redacted at the source before this leaves the remote host.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpExchange {
    /// Unique id for this exchange (uuid).
    pub exchange_id: String,
    pub method: String,
    /// Full request URL (scheme://host/path?query).
    pub url: String,
    /// Host portion, for quick grouping in the UI.
    pub host: String,
    pub req_headers: std::collections::HashMap<String, String>,
    #[serde(with = "serde_bytes")]
    pub req_body: Vec<u8>,
    pub status: u16,
    pub resp_headers: std::collections::HashMap<String, String>,
    #[serde(with = "serde_bytes")]
    pub resp_body: Vec<u8>,
    /// Unix epoch millis when the request started.
    pub started_at: u64,
    pub duration_ms: u64,
    pub mode: TapMode,
    /// True if either body was truncated at the capture cap.
    pub truncated: bool,
}

