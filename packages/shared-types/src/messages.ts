// Wire-protocol message types for the frontend.
// Mirrors crates/shared-protocol/src/messages.rs ProtocolMessage enum.
// Uses discriminated unions with a `type` tag for message routing.

import type {
  ToolKind,
  SessionState,
  SessionEventType,
  ErrorCode,
  PauseReason,
} from './types';

// ── Connection Establishment ──────────────────────────────

export interface HelloMessage {
  type: 'hello';
  payload: {
    version: number;
    capabilities: string[];
    session_id: string;
  };
}

export interface HelloAckMessage {
  type: 'hello_ack';
  payload: {
    version: number;
    server_version: string;
    server_arch: string;
  };
}

export interface ErrorMessage {
  type: 'error';
  payload: {
    code: ErrorCode;
    message: string;
    session_id?: string;
  };
}

export interface GoodbyeMessage {
  type: 'goodbye';
  payload: {
    reason?: string;
  };
}

// ── Session Lifecycle ─────────────────────────────────────

export interface SpawnSessionMessage {
  type: 'spawn_session';
  payload: {
    session_id: string;
    tool: ToolKind;
    args: string[];
    env?: Record<string, string>;
    cwd?: string;
    terminal_cols: number;
    terminal_rows: number;
  };
}

export interface SpawnSessionAckMessage {
  type: 'spawn_session_ack';
  payload: {
    session_id: string;
    pid: number;
    tool_version?: string;
  };
}

export interface SpawnSessionNackMessage {
  type: 'spawn_session_nack';
  payload: {
    session_id: string;
    reason: string;
  };
}

export interface CloseSessionMessage {
  type: 'close_session';
  payload: {
    session_id: string;
  };
}

export interface CloseSessionAckMessage {
  type: 'close_session_ack';
  payload: {
    session_id: string;
    exit_code?: number;
  };
}

// ── Terminal I/O ──────────────────────────────────────────

export interface TerminalDataMessage {
  type: 'terminal_data';
  payload: {
    session_id: string;
    data: number[]; // Uint8Array serialized as number array in JSON
    seq: number;
  };
}

export interface TerminalInputMessage {
  type: 'terminal_input';
  payload: {
    session_id: string;
    data: number[];
  };
}

export interface TerminalResizeMessage {
  type: 'terminal_resize';
  payload: {
    session_id: string;
    cols: number;
    rows: number;
  };
}

// ── Flow Control ──────────────────────────────────────────

export interface AckMessage {
  type: 'ack';
  payload: {
    session_id: string;
    seq: number;
    bytes_consumed: number;
  };
}

export interface PauseMessage {
  type: 'pause';
  payload: {
    session_id: string;
    reason: PauseReason;
  };
}

export interface ResumeMessage {
  type: 'resume';
  payload: {
    session_id: string;
  };
}

// ── Structured Session Events ─────────────────────────────

export interface SessionEventMessage {
  type: 'session_event';
  payload: {
    session_id: string;
    event_type: SessionEventType;
    data?: Record<string, string>;
    timestamp: number;
  };
}

// ── Tool Management ───────────────────────────────────────

export interface ProbeRequestMessage {
  type: 'probe_request';
  payload: {
    tool: ToolKind;
  };
}

export interface ProbeResponseMessage {
  type: 'probe_response';
  payload: {
    tool: ToolKind;
    installed: boolean;
    version?: string;
    path?: string;
    auth_ok?: boolean;
    details?: Record<string, string>;
  };
}

export interface InstallRequestMessage {
  type: 'install_request';
  payload: {
    tool: ToolKind;
    version?: string;
  };
}

export interface InstallProgressMessage {
  type: 'install_progress';
  payload: {
    tool: ToolKind;
    phase: string;
    progress: number;
    message: string;
  };
}

export interface InstallCompleteMessage {
  type: 'install_complete';
  payload: {
    tool: ToolKind;
    success: boolean;
    version?: string;
    error?: string;
  };
}

// ── Keepalive ─────────────────────────────────────────────

export interface PingMessage {
  type: 'ping';
  payload: {
    nonce: number;
  };
}

export interface PongMessage {
  type: 'pong';
  payload: {
    nonce: number;
  };
}

// ── Union Type ────────────────────────────────────────────

export type ProtocolMessage =
  | HelloMessage
  | HelloAckMessage
  | ErrorMessage
  | GoodbyeMessage
  | SpawnSessionMessage
  | SpawnSessionAckMessage
  | SpawnSessionNackMessage
  | CloseSessionMessage
  | CloseSessionAckMessage
  | TerminalDataMessage
  | TerminalInputMessage
  | TerminalResizeMessage
  | AckMessage
  | PauseMessage
  | ResumeMessage
  | SessionEventMessage
  | ProbeRequestMessage
  | ProbeResponseMessage
  | InstallRequestMessage
  | InstallProgressMessage
  | InstallCompleteMessage
  | PingMessage
  | PongMessage;

/** All message type strings for runtime checks. */
export const MESSAGE_TYPES = [
  'hello', 'hello_ack', 'error', 'goodbye',
  'spawn_session', 'spawn_session_ack', 'spawn_session_nack',
  'close_session', 'close_session_ack',
  'terminal_data', 'terminal_input', 'terminal_resize',
  'ack', 'pause', 'resume',
  'session_event',
  'probe_request', 'probe_response',
  'install_request', 'install_progress', 'install_complete',
  'ping', 'pong',
] as const;

export type MessageType = (typeof MESSAGE_TYPES)[number];

/** Extract a session ID from a message if it carries one. */
export function messageSessionId(msg: ProtocolMessage): string | undefined {
  const p = msg.payload as Record<string, unknown>;
  return typeof p.session_id === 'string' ? p.session_id : undefined;
}

/** Returns a human-readable label for a message type. */
export function messageKind(msg: ProtocolMessage): string {
  return msg.type;
}
