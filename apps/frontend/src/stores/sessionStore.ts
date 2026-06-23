/**
 * Session Store — Zustand store for session state.
 */
import { create } from 'zustand';
import type { SessionInfo, SpawnRequest } from '../api/types';
import { createTerminalApi } from '../api/terminalApi';

const api = createTerminalApi();

interface SessionStore {
  sessions: Record<string, SessionInfo>;
  activeSessionId: string | null;

  spawn: (connectionId: string, req: SpawnRequest) => Promise<SessionInfo>;
  close: (sessionId: string) => Promise<void>;
  write: (sessionId: string, data: string) => void;
  resize: (sessionId: string, cols: number, rows: number) => void;
  setActive: (sessionId: string | null) => void;
  _addSession: (info: SessionInfo) => void;
  _removeSession: (id: string) => void;
  _updateSession: (id: string, partial: Partial<SessionInfo>) => void;
}

export const useSessionStore = create<SessionStore>((set, get) => ({
  sessions: {},
  activeSessionId: null,

  async spawn(connectionId: string, req: SpawnRequest): Promise<SessionInfo> {
    const info = await api.spawn(connectionId, req);
    set((s) => ({
      sessions: { ...s.sessions, [info.id]: info },
      activeSessionId: info.id,
    }));
    return info;
  },

  async close(sessionId: string): Promise<void> {
    // Try to notify agent, but always clean up local state even if agent is gone
    let _ = api.close(sessionId);
    set((s) => {
      const next = { ...s.sessions };
      delete next[sessionId];
      return {
        sessions: next,
        activeSessionId: s.activeSessionId === sessionId ? null : s.activeSessionId,
      };
    });
  },

  write(sessionId: string, data: string) {
    api.write(sessionId, data);
  },

  resize(sessionId: string, cols: number, rows: number) {
    api.resize(sessionId, cols, rows);
  },

  setActive(sessionId: string | null) {
    set({ activeSessionId: sessionId });
  },

  _addSession(info: SessionInfo) {
    set((s) => ({ sessions: { ...s.sessions, [info.id]: info } }));
  },

  _removeSession(id: string) {
    set((s) => {
      const next = { ...s.sessions };
      delete next[id];
      return {
        sessions: next,
        activeSessionId: s.activeSessionId === id ? null : s.activeSessionId,
      };
    });
  },

  _updateSession(id: string, partial: Partial<SessionInfo>) {
    set((s) => {
      const cur = s.sessions[id];
      if (!cur) return s;
      return { sessions: { ...s.sessions, [id]: { ...cur, ...partial } } };
    });
  },
}));
