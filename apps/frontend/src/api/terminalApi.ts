/**
 * Terminal API — abstraction over Tauri IPC for terminal I/O.
 *
 * Commands use `invoke()` to call Rust command handlers.
 * Streaming data comes via `listen()` for Tauri events.
 */
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { SessionInfo, SpawnRequest } from './types';

export interface TerminalApi {
  spawn(connectionId: string, req: SpawnRequest): Promise<SessionInfo>;
  write(sessionId: string, data: string): Promise<void>;
  resize(sessionId: string, cols: number, rows: number): Promise<void>;
  close(sessionId: string): Promise<void>;

  onData(cb: (sessionId: string, data: Uint8Array, seq: number) => void): UnlistenFn;
  onSessionEvent(cb: (sessionId: string, eventType: string, data: Record<string, string>) => void): UnlistenFn;
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
  };
}
