// Frontend-specific types that wrap protocol types with UI concerns.
// Re-exports from shared-types for convenience.

export type {
  ToolKind,
  SessionState,
  SessionEventType,
  ErrorCode,
  SessionMetadata,
  SessionSummary,
  RemoteHostInfo,
  ProbeResult,
  CostBreakdown,
} from 'shared-types';

export type {
  ProtocolMessage,
  TerminalDataMessage,
  SessionEventMessage,
  SpawnSessionMessage,
} from 'shared-types';

/** Connection configuration for the frontend. */
export interface ConnectionConfig {
  label?: string;
  host: string;
  port: number;
  user: string;
  authMethod: 'key' | 'agent' | 'password' | 'config';
  identityFile?: string;
  password?: string;
  sshConfigHost?: string;
}

/** Connection information from the backend. */
export interface ConnectionInfo {
  id: string;
  label: string;
  host: string;
  port: number;
  user: string;
  status: 'disconnected' | 'connecting' | 'bootstrapping' | 'connected' | 'reconnecting' | 'error';
  error?: string;
  remoteInfo?: {
    arch: string;
    agentVersion: string;
  };
}

/** Session information from the backend. */
export interface SessionInfo {
  id: string;
  connectionId: string;
  tool: string;
  toolVersion?: string;
  state: 'spawning' | 'running' | 'paused' | 'ended';
  pid?: number;
  cols: number;
  rows: number;
  createdAt: string;
  endedAt?: string;
  exitCode?: number;
  metadata: {
    cwd?: string;
    gitBranch?: string;
    gitRepo?: string;
    args: string[];
  };
  cost: {
    inputTokens: number;
    outputTokens: number;
    cacheReadTokens: number;
    cacheWriteTokens: number;
    costUsd: number;
  };
  turnCount: number;
}

/** Request to spawn a new session. */
export interface SpawnRequest {
  tool: string;
  args?: string[];
  cwd?: string;
  env?: Record<string, string>;
  container?: string;
}
