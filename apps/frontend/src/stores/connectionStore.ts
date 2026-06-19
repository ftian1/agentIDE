/**
 * Connection Store — Zustand store for SSH connection state.
 */
import { create } from 'zustand';
import type { ConnectionConfig, ConnectionInfo } from '../api/types';

interface ConnectionStore {
  connections: Record<string, ConnectionInfo>;
  activeConnectionId: string | null;

  // Actions
  connect: (config: ConnectionConfig) => Promise<string>;
  disconnect: (id: string) => Promise<void>;
  setActive: (id: string | null) => void;
  removeConnection: (id: string) => void;

  // Mutations
  _addConnection: (info: ConnectionInfo) => void;
  _updateStatus: (id: string, status: ConnectionInfo['status'], error?: string) => void;

  // Computed
  activeConnection: () => ConnectionInfo | null;
}

export const useConnectionStore = create<ConnectionStore>((set, get) => ({
  connections: {},
  activeConnectionId: null,

  async connect(_config: ConnectionConfig): Promise<string> {
    throw new Error('ConnectionStore.connect: not yet implemented (Phase 3)');
  },

  async disconnect(_id: string): Promise<void> {
    throw new Error('ConnectionStore.disconnect: not yet implemented (Phase 3)');
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
