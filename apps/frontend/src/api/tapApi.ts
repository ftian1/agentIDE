/**
 * Tap API — Tauri command wrappers for the HTTP traffic tap.
 */
import { invoke } from '@tauri-apps/api/core';

export interface TapSettings {
  enabled: boolean;
  /** "mitm" | "reverse" */
  mode: 'mitm' | 'reverse';
}

export interface TapApi {
  loadSettings: () => Promise<TapSettings | null>;
  saveSettings: (settings: TapSettings) => Promise<void>;
  readTraces: (connectionId: string) => Promise<unknown[]>;
  clearTraces: (connectionId: string) => Promise<void>;
  loadExchangesDb: (connectionId: string, limit?: number, offset?: number) => Promise<unknown[]>;
  clearExchangesDb: (connectionId: string) => Promise<void>;
}

export function createTapApi(): TapApi {
  return {
    loadSettings: () => invoke<TapSettings | null>('load_tap_settings'),
    saveSettings: (settings) => invoke<void>('save_tap_settings', { settings }),
    readTraces: (connectionId) => invoke<unknown[]>('read_tap_traces', { connectionId }),
    clearTraces: (connectionId) => invoke<void>('clear_tap_traces', { connectionId }),
    loadExchangesDb: (connectionId, limit, offset) =>
      invoke<unknown[]>('load_tap_exchanges_db', { connectionId, limit, offset }),
    clearExchangesDb: (connectionId) =>
      invoke<void>('clear_tap_exchanges_db', { connectionId }),
  };
}
