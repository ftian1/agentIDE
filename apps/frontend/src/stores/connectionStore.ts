/**
 * Connection Store — Zustand store for SSH connection state.
 */
import { create } from 'zustand';
import { createConnectionApi } from '../api/connectionApi';
import type { ConnectionConfig, ConnectionInfo } from '../api/types';

const api = createConnectionApi();

interface ConnectionStore {
  connections: Record<string, ConnectionInfo>;
  activeConnectionId: string | null;

  connect: (config: ConnectionConfig) => Promise<ConnectionInfo>;
  disconnect: (id: string) => Promise<void>;
  setActive: (id: string | null) => void;
  removeConnection: (id: string) => void;

  _addConnection: (info: ConnectionInfo) => void;
  _updateStatus: (id: string, status: ConnectionInfo['status'], error?: string) => void;

  activeConnection: () => ConnectionInfo | null;
}

export const useConnectionStore = create<ConnectionStore>((set, get) => ({
  connections: {},
  activeConnectionId: null,

  async connect(config: ConnectionConfig): Promise<ConnectionInfo> {
    const info = await api.connect(config);
    set((s) => ({
      connections: { ...s.connections, [info.id]: info },
      activeConnectionId: info.id,
    }));
    return info;
  },

  async disconnect(id: string): Promise<void> {
    await api.disconnect(id);
    set((s) => {
      const next = { ...s.connections };
      delete next[id];
      return {
        connections: next,
        activeConnectionId: s.activeConnectionId === id ? null : s.activeConnectionId,
      };
    });
  },

  setActive(id: string | null) {
    set({ activeConnectionId: id });
  },

  removeConnection(id: string) {
    set((s) => {
      const next = { ...s.connections };
      delete next[id];
      return {
        connections: next,
        activeConnectionId: s.activeConnectionId === id ? null : s.activeConnectionId,
      };
    });
  },

  _addConnection(info: ConnectionInfo) {
    set((s) => ({
      connections: { ...s.connections, [info.id]: info },
    }));
  },

  _updateStatus(id: string, status: ConnectionInfo['status'], error?: string) {
    set((s) => {
      const existing = s.connections[id];
      if (!existing) return s;
      return {
        connections: { ...s.connections, [id]: { ...existing, status, error } },
      };
    });
  },

  activeConnection() {
    const { connections, activeConnectionId } = get();
    return activeConnectionId ? connections[activeConnectionId] ?? null : null;
  },
}));
