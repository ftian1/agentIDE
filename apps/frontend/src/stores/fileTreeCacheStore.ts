/**
 * File Tree Cache Store — pre-reads remote file system listings when an SSH
 * connection becomes ready, so the file explorer renders instantly without a
 * loading flash.
 *
 * The cache is populated by a connectionStore subscriber registered in main.tsx
 * via initFileTreeListeners(). It runs before React mounts, so pre-reads fire
 * even when the explorer panel isn't visible yet.
 */
import { create } from 'zustand';
import { useConnectionStore } from './connectionStore';
import { createTerminalApi } from '../api/terminalApi';
import type { FileEntry } from '../api/terminalApi';

/* ------------------------------------------------------------------ */
/*  Types                                                              */
/* ------------------------------------------------------------------ */

interface ConnectionFileCache {
  rootPath: string;
  rootEntries: FileEntry[];
  loadedAt: number;
}

export interface FileTreeCacheStore {
  caches: Record<string, ConnectionFileCache>;
  loading: Record<string, boolean>;
  errors: Record<string, string>;

  /** Atomically mark a connection as "loading" so concurrent triggers don't
   *  start duplicate fetches. */
  startLoad: (connectionId: string) => void;

  /** Store a successful pre-read. Clears loading + error for this connection. */
  setCache: (connectionId: string, rootPath: string, entries: FileEntry[]) => void;

  /** Store a pre-read failure. Clears loading for this connection. */
  setError: (connectionId: string, err: string) => void;

  /** Clear cache + loading + error for a connection (on disconnect). */
  clearCache: (connectionId: string) => void;
}

/* ------------------------------------------------------------------ */
/*  Store                                                              */
/* ------------------------------------------------------------------ */

export const useFileTreeCacheStore = create<FileTreeCacheStore>((set) => ({
  caches: {},
  loading: {},
  errors: {},

  startLoad(connectionId: string) {
    set((s) => ({
      loading: { ...s.loading, [connectionId]: true },
      errors: (() => {
        const next = { ...s.errors };
        delete next[connectionId];
        return next;
      })(),
    }));
  },

  setCache(connectionId: string, rootPath: string, entries: FileEntry[]) {
    set((s) => ({
      caches: {
        ...s.caches,
        [connectionId]: { rootPath, rootEntries: entries, loadedAt: Date.now() },
      },
      loading: (() => {
        const next = { ...s.loading };
        delete next[connectionId];
        return next;
      })(),
      errors: (() => {
        const next = { ...s.errors };
        delete next[connectionId];
        return next;
      })(),
    }));
  },

  setError(connectionId: string, err: string) {
    set((s) => ({
      errors: { ...s.errors, [connectionId]: err },
      loading: (() => {
        const next = { ...s.loading };
        delete next[connectionId];
        return next;
      })(),
    }));
  },

  clearCache(connectionId: string) {
    set((s) => {
      const caches = { ...s.caches };
      delete caches[connectionId];
      const loading = { ...s.loading };
      delete loading[connectionId];
      const errors = { ...s.errors };
      delete errors[connectionId];
      return { caches, loading, errors };
    });
  },
}));

/* ------------------------------------------------------------------ */
/*  Pre-read subscriber (called from main.tsx before React mounts)     */
/* ------------------------------------------------------------------ */

let subscribed = false;

/**
 * Subscribe to connectionStore and pre-read the home directory whenever a
 * connection transitions to `connected`. Idempotent — safe to call multiple
 * times; only subscribes once.
 */
export function initFileTreeListeners() {
  if (subscribed) return;
  subscribed = true;

  const api = createTerminalApi();

  // Zustand v5 subscribe: listener receives (state, prevState)
  useConnectionStore.subscribe(
    (state, prevState) => {
      const curr = state.connections;
      const prev = prevState?.connections;

      for (const [id, conn] of Object.entries(curr)) {
        const prevStatus = prev?.[id]?.status;
        const currStatus = conn.status;

        // Detect transition to connected
        if (prevStatus !== 'connected' && currStatus === 'connected') {
          triggerPreRead(api, id, conn.user);
        }

        // Detect transition away from connected → evict cache
        if (prevStatus === 'connected' && currStatus !== 'connected') {
          useFileTreeCacheStore.getState().clearCache(id);
        }
      }

      // Also clear caches for removed connections
      if (prev) {
        for (const id of Object.keys(prev)) {
          if (!curr[id]) {
            useFileTreeCacheStore.getState().clearCache(id);
          }
        }
      }
    },
  );
}

async function triggerPreRead(
  api: ReturnType<typeof createTerminalApi>,
  connectionId: string,
  user: string,
) {
  const store = useFileTreeCacheStore.getState();

  // Guard: already loading or already cached
  if (store.loading[connectionId] || store.caches[connectionId]) {
    return;
  }

  store.startLoad(connectionId);

  const homeDir = user === 'root' ? '/root' : `/home/${user}`;
  try {
    const entries = await api.listFiles(connectionId, homeDir);

    // Only store if connection is still connected (not torn down during fetch)
    const stillConnected =
      useConnectionStore.getState().connections[connectionId]?.status === 'connected';
    if (stillConnected) {
      useFileTreeCacheStore.getState().setCache(connectionId, homeDir, entries);
    }
  } catch (err) {
    useFileTreeCacheStore
      .getState()
      .setError(connectionId, err instanceof Error ? err.message : String(err));
  }
}
