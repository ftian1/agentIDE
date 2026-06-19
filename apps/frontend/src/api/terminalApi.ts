/**
 * Terminal API — abstraction over Tauri IPC for terminal I/O + code changes.
 */
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { SessionInfo, SpawnRequest } from './types';

export interface CodeChangeEvent {
  session_id: string;
  change_set_id: string;
  change_id: string;
  file_path: string;
  old_content: string | null;
  new_content: string | null;
  diff: string;
}

export interface CodeChangeBatchEvent {
  session_id: string;
  change_set_id: string;
  description: string;
  status: string;
  file_count: number;
}

export interface TerminalApi {
  spawn(connectionId: string, req: SpawnRequest): Promise<SessionInfo>;
  write(sessionId: string, data: string): Promise<void>;
  resize(sessionId: string, cols: number, rows: number): Promise<void>;
  close(sessionId: string): Promise<void>;

  applyChange(sessionId: string, filePath: string, content: string): Promise<void>;
  rejectChange(changeId: string): Promise<void>;

  onData(cb: (sessionId: string, data: Uint8Array, seq: number) => void): UnlistenFn;
  onSessionEvent(cb: (sessionId: string, eventType: string, data: Record<string, string>) => void): UnlistenFn;
  onCodeChange(cb: (event: CodeChangeEvent) => void): UnlistenFn;
  onCodeChangeBatch(cb: (event: CodeChangeBatchEvent) => void): UnlistenFn;
}

export function createTerminalApi(): TerminalApi {
  return {
    async spawn(connectionId: string, req: SpawnRequest): Promise<SessionInfo> {
      return invoke<SessionInfo>('spawn_session', {
        connectionId,
        req: {
          tool: req.tool,
          args: req.args ?? [],
          cwd: req.cwd ?? null,
          env: req.env ?? null,
          terminal_cols: 80,
          terminal_rows: 24,
        },
      });
    },

    async write(sessionId: string, data: string): Promise<void> {
      const bytes = new TextEncoder().encode(data);
      await invoke('write_input', {
        connectionId: '',
        sessionId,
        data: Array.from(bytes),
      });
    },

    async resize(sessionId: string, cols: number, rows: number): Promise<void> {
      await invoke('resize_terminal', {
        connectionId: '',
        sessionId,
        cols,
        rows,
      });
    },

    async close(sessionId: string): Promise<void> {
      await invoke('close_session', {
        connectionId: '',
        sessionId,
      });
    },

    async applyChange(sessionId: string, filePath: string, content: string): Promise<void> {
      await invoke('apply_code_change', {
        sessionId,
        filePath,
        content,
      });
    },

    async rejectChange(_changeId: string): Promise<void> {
      await invoke('reject_code_change', {
        changeId: _changeId,
      });
    },

    onData(cb: (sessionId: string, data: Uint8Array, seq: number) => void): UnlistenFn {
      let unlisten: UnlistenFn | null = null;
      listen<{ session_id: string; data: number[]; seq: number }>('terminal:data', (event) => {
        const { session_id, data, seq } = event.payload;
        cb(session_id, new Uint8Array(data), seq);
      }).then((fn) => { unlisten = fn; });
      return () => { unlisten?.(); };
    },

    onSessionEvent(
      cb: (sessionId: string, eventType: string, data: Record<string, string>) => void,
    ): UnlistenFn {
      let unlisten: UnlistenFn | null = null;
      listen<{ session_id: string; event_type: string; data: Record<string, string> }>(
        'session:event',
        (event) => {
          const { session_id, event_type, data } = event.payload;
          cb(session_id, event_type, data ?? {});
        },
      ).then((fn) => { unlisten = fn; });
      return () => { unlisten?.(); };
    },

    onCodeChange(cb: (event: CodeChangeEvent) => void): UnlistenFn {
      let unlisten: UnlistenFn | null = null;
      listen<CodeChangeEvent>('code:change', (event) => {
        cb(event.payload);
      }).then((fn) => { unlisten = fn; });
      return () => { unlisten?.(); };
    },

    onCodeChangeBatch(cb: (event: CodeChangeBatchEvent) => void): UnlistenFn {
      let unlisten: UnlistenFn | null = null;
      listen<CodeChangeBatchEvent>('code:change_batch', (event) => {
        cb(event.payload);
      }).then((fn) => { unlisten = fn; });
      return () => { unlisten?.(); };
    },
  };
}
