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

export interface FileEntry {
  name: string;
  path: string;
  kind: 'file' | 'directory';
}

export interface TerminalApi {
  spawn(connectionId: string, req: SpawnRequest): Promise<SessionInfo>;
  write(sessionId: string, data: string): Promise<void>;
  resize(sessionId: string, cols: number, rows: number): Promise<void>;
  close(sessionId: string): Promise<void>;
  listFiles(connectionId: string, path: string): Promise<FileEntry[]>;

  applyChange(sessionId: string, filePath: string, content: string): Promise<void>;
  rejectChange(changeId: string): Promise<void>;

  onData(cb: (sessionId: string, data: Uint8Array, seq: number) => void): UnlistenFn;
  onSessionEvent(cb: (sessionId: string, eventType: string, data: Record<string, string>) => void): UnlistenFn;
  onCodeChange(cb: (event: CodeChangeEvent) => void): UnlistenFn;
  onCodeChangeBatch(cb: (event: CodeChangeBatchEvent) => void): UnlistenFn;
}

export function createTerminalApi(): TerminalApi {
  // Eagerly listen for terminal:data events and buffer per session_id.
  // Solves the race: relay emits events BEFORE TerminalInstance mounts.
  const dataBuffers = new Map<string, Array<{ data: Uint8Array; seq: number }>>();
  const dataListeners = new Set<(sid: string, data: Uint8Array, seq: number) => void>();

  // Register the global listener ONCE, immediately — before any session exists.
  listen<{ session_id: string; data: number[]; seq: number }>('terminal:data', (event) => {
    const { session_id, data, seq } = event.payload;
    const u8 = new Uint8Array(data);

    // Notify all registered callbacks (active TerminalInstances)
    for (const cb of dataListeners) {
      cb(session_id, u8, seq);
    }

    // Also buffer for late-arriving TerminalInstances
    let buf = dataBuffers.get(session_id);
    if (!buf) {
      buf = [];
      dataBuffers.set(session_id, buf);
    }
    buf.push({ data: u8, seq });
  });

  return {
    async spawn(connectionId: string, req: SpawnRequest): Promise<SessionInfo> {
      console.log(`[api] spawn: conn=${connectionId} tool=${req.tool} args=${JSON.stringify(req.args)} cwd=${req.cwd} container=${req.container}`);
      const info = await invoke<SessionInfo>('spawn_session', {
        connectionId,
        req: {
          tool: req.tool,
          args: req.args ?? [],
          cwd: req.cwd ?? null,
          env: req.env ?? null,
          container: req.container ?? null,
          terminal_cols: 80,
          terminal_rows: 24,
        },
      });
      console.log(`[api] spawn OK: session=${info.id} conn=${info.connectionId} tool=${info.tool} pid=${info.pid}`);
      return info;
    },

    async write(sessionId: string, data: string): Promise<void> {
      const bytes = new TextEncoder().encode(data);
      console.log(`[api] write: session=${sessionId} len=${bytes.length} preview=${JSON.stringify(data.slice(0, 20))}`);
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

    async listFiles(connectionId: string, path: string): Promise<FileEntry[]> {
      return invoke<FileEntry[]>('list_files', { connectionId, path });
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
      // Replay buffered data immediately (data that arrived before mount)
      for (const [sid, events] of dataBuffers) {
        for (const ev of events) {
          cb(sid, ev.data, ev.seq);
        }
      }
      // Register for live events
      dataListeners.add(cb);
      return () => { dataListeners.delete(cb); };
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
