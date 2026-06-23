/**
 * Perf Store — latest remote performance sample per connection.
 *
 * Fed by `perf:stats` events emitted from the Rust perf monitor. Powers the
 * status bar's "CPU / MEM / Disk IO" readout.
 */
import { create } from 'zustand';

export interface PerfStats {
  connectionId: string;
  cpuPercent: number;
  memUsedMb: number;
  memTotalMb: number;
  diskIo: string;
  diskSectorsPerSec: number;
}

interface PerfStore {
  byConnection: Record<string, PerfStats>;
  _set: (stats: PerfStats) => void;
  clear: (connectionId: string) => void;
}

export const usePerfStore = create<PerfStore>((set) => ({
  byConnection: {},
  _set: (stats) =>
    set((s) => ({ byConnection: { ...s.byConnection, [stats.connectionId]: stats } })),
  clear: (connectionId) =>
    set((s) => {
      const next = { ...s.byConnection };
      delete next[connectionId];
      return { byConnection: next };
    }),
}));

/** Format MiB as a human string: 3200 → "3.2G", 512 → "512M". */
export function formatMem(mb: number): string {
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)}G`;
  return `${mb}M`;
}

/** Initialize the perf:stats listener. Call once at app startup. */
export function initPerfListeners() {
  import('@tauri-apps/api/event')
    .then(({ listen }) => {
      listen<{
        connectionId: string;
        cpuPercent: number;
        memUsedMb: number;
        memTotalMb: number;
        diskIo: string;
        diskSectorsPerSec: number;
      }>('perf:stats', (event) => {
        usePerfStore.getState()._set(event.payload);
      });
    })
    .catch(() => {
      // Tauri API not available (browser dev mode).
    });
}
