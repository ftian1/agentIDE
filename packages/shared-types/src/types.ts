// Core types shared across frontend packages.
// Mirrors crates/shared-protocol/src/types.rs enums and structs.

// ── Tool Kind ────────────────────────────────────────────

export type ToolKind = 'claude' | 'copilot' | { custom: string };

// ── Session State ────────────────────────────────────────

export type SessionState =
  | 'spawning'
  | 'running'
  | { paused: PauseReason }
  | { ended: EndReason };

export type PauseReason = 'flow_control' | 'user_request' | { error: string };

export type EndReason =
  | 'user_closed'
  | { process_exited: number }
  | { crashed: string }
  | 'connection_lost'
  | 'timed_out';

// ── Session Event Type ───────────────────────────────────

export type SessionEventType =
  | 'turn_start'
  | 'turn_end'
  | 'tool_call'
  | 'tool_result'
  | 'subagent_spawned'
  | 'subagent_ended'
  | 'shell_command'
  | 'cost_update'
  | 'token_metric'
  | 'error'
  | 'warning'
  | 'info';

// ── Error Code ───────────────────────────────────────────

export type ErrorCode =
  | 'invalid_message'
  | 'session_not_found'
  | 'session_already_exists'
  | 'spawn_failed'
  | 'pty_error'
  | 'transport_error'
  | 'bootstrap_failed'
  | 'tool_not_found'
  | 'install_failed'
  | 'auth_failed'
  | 'internal_error'
  | 'timeout'
  | 'unsupported_architecture';

// ── Session Metadata ─────────────────────────────────────

export interface SessionMetadata {
  cwd?: string;
  env: Record<string, string>;
  terminal_cols: number;
  terminal_rows: number;
  args: string[];
}

// ── Session Summary ──────────────────────────────────────

export interface SessionSummary {
  session_id: string;
  tool: ToolKind;
  state: SessionState;
  pid: number;
  created_at: number; // unix timestamp millis
  turn_count: number;
}

// ── Remote Host Info ─────────────────────────────────────

export interface RemoteHostInfo {
  arch: string;
  platform: string;
  home_dir: string;
  user: string;
  agent_version?: string;
}

// ── Probe Result ─────────────────────────────────────────

export interface ProbeResult {
  tool: ToolKind;
  installed: boolean;
  version?: string;
  path?: string;
  auth_ok?: boolean;
  details?: Record<string, string>;
}

// ── Cost Breakdown ───────────────────────────────────────

export interface CostBreakdown {
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_write_tokens: number;
  cost_usd: number;
}
