/**
 * Connection API — wrappers around Tauri IPC for SSH connection management.
 */
import type { ConnectionConfig, ConnectionInfo } from './types';

export interface ConnectionApi {
  connect(config: ConnectionConfig): Promise<ConnectionInfo>;
  disconnect(connectionId: string): Promise<void>;
  listConnections(): Promise<ConnectionInfo[]>;
  listSshConfigs(): Promise<Record<string, unknown>[]>;
}

/** Create a ConnectionApi backed by Tauri IPC. */
export function createConnectionApi(): ConnectionApi {
  return {
    async connect(_config: ConnectionConfig): Promise<ConnectionInfo> {
      throw new Error('ConnectionApi: not yet implemented (Phase 3)');
    },
    async disconnect(_connectionId: string): Promise<void> {
      throw new Error('ConnectionApi: not yet implemented (Phase 3)');
    },
    async listConnections(): Promise<ConnectionInfo[]> {
      return [];
    },
    async listSshConfigs(): Promise<Record<string, unknown>[]> {
      return [];
    },
  };
}
