/**
 * Connection API — wraps Tauri IPC for SSH connection management.
 */
import { invoke } from '@tauri-apps/api/core';
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
    async connect(config: ConnectionConfig): Promise<ConnectionInfo> {
      return invoke<ConnectionInfo>('connect', { req: config });
    },
    async disconnect(connectionId: string): Promise<void> {
      return invoke('disconnect', { connectionId });
    },
    async listConnections(): Promise<ConnectionInfo[]> {
      return invoke<ConnectionInfo[]>('list_connections');
    },
    async listSshConfigs(): Promise<Record<string, unknown>[]> {
      return invoke<Record<string, unknown>[]>('list_ssh_configs');
    },
  };
}
