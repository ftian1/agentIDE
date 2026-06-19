/**
 * Bootstrap Store — Zustand store for remote host bootstrap progress.
 */
import { create } from 'zustand';

export interface BootstrapProgress {
  connectionId: string;
  phase: 'detecting' | 'uploading' | 'starting' | 'handshaking' | 'complete';
  progress: number; // 0.0 - 1.0
  message: string;
  error?: string;
}

interface BootstrapStore {
  progress: Record<string, BootstrapProgress>;

  _setProgress: (connectionId: string, p: BootstrapProgress) => void;
  _clearProgress: (connectionId: string) => void;
}

export const useBootstrapStore = create<BootstrapStore>((set) => ({
  progress: {},

  _setProgress(connectionId: string, p: BootstrapProgress) {
    set((s) => ({
      progress: { ...s.progress, [connectionId]: p },
    }));
  },

  _clearProgress(connectionId: string) {
    set((s) => {
      const next = { ...s.progress };
      delete next[connectionId];
      return { progress: next };
    });
  },
}));
