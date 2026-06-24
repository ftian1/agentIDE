/**
 * HTTP Traffic Store — captured agent CLI request/response exchanges.
 *
 * Fed by the `http:traffic` Tauri event (one per completed exchange) emitted by
 * the desktop demux relay. Also loads persisted JSONL traces on demand.
 *
 * Selector discipline: components select `s.exchanges` (stable ref) and derive
 * with useMemo — returning a fresh array from a selector triggers React #185.
 */
import { create } from 'zustand';
import { createTapApi, type TapSettings } from '../api/tapApi';

/** Mirrors the Rust HttpExchange (camelCase; bodies arrive as byte arrays). */
export interface HttpExchange {
  exchangeId: string;
  method: string;
  url: string;
  host: string;
  reqHeaders: Record<string, string>;
  reqBody: number[];
  status: number;
  respHeaders: Record<string, string>;
  respBody: number[];
  startedAt: number;
  durationMs: number;
  mode: 'mitm' | 'reverse';
  truncated: boolean;
}

export interface TrafficRecord {
  sessionId: string;
  connectionId: string;
  seq: number;
  exchange: HttpExchange;
}

const MAX_RECORDS = 2000;
const api = createTapApi();

const DEFAULT_SETTINGS: TapSettings = { enabled: true, mode: 'mitm' };

interface HttpTrafficStore {
  exchanges: TrafficRecord[];
  settings: TapSettings;
  loaded: boolean;

  loadSettings: () => Promise<void>;
  setSettings: (next: TapSettings) => Promise<void>;
  loadTraces: (connectionId: string) => Promise<void>;
  clear: (connectionId?: string) => void;
  _append: (rec: TrafficRecord) => void;
}

export const useHttpTrafficStore = create<HttpTrafficStore>((set, get) => ({
  exchanges: [],
  settings: DEFAULT_SETTINGS,
  loaded: false,

  async loadSettings() {
    try {
      const s = await api.loadSettings();
      set({ settings: s ?? DEFAULT_SETTINGS, loaded: true });
      return;
    } catch {
      // Tauri unavailable.
    }
    set({ loaded: true });
  },

  async setSettings(next) {
    set({ settings: next });
    try {
      await api.saveSettings(next);
    } catch {
      // ignore in dev
    }
  },

  async loadTraces(connectionId) {
    try {
      const rows = (await api.readTraces(connectionId)) as Array<{
        session_id?: string;
        sessionId?: string;
        connection_id?: string;
        connectionId?: string;
        seq: number;
        exchange: HttpExchange;
      }>;
      const recs: TrafficRecord[] = rows.map((r) => ({
        sessionId: r.sessionId ?? r.session_id ?? '',
        connectionId: r.connectionId ?? r.connection_id ?? connectionId,
        seq: r.seq,
        exchange: r.exchange,
      }));
      set({ exchanges: recs.slice(-MAX_RECORDS) });
    } catch {
      // no traces / Tauri unavailable
    }
  },

  clear() {
    set({ exchanges: [] });
  },

  _append(rec) {
    set((s) => {
      const next = [...s.exchanges, rec];
      if (next.length > MAX_RECORDS) next.splice(0, next.length - MAX_RECORDS);
      return { exchanges: next };
    });
  },
}));

export function initHttpTrafficListeners() {
  import('@tauri-apps/api/event').then(({ listen }) => {
    listen<{
      session_id: string;
      connection_id: string;
      seq: number;
      exchange: HttpExchange;
    }>('http:traffic', (event) => {
      const p = event.payload;
      useHttpTrafficStore.getState()._append({
        sessionId: p.session_id,
        connectionId: p.connection_id,
        seq: p.seq,
        exchange: p.exchange,
      });
    });
  });
  // Pull persisted settings so the toggle reflects the saved state.
  useHttpTrafficStore.getState().loadSettings();
}
