/**
 * File Buffer Store — open file buffers backed by remote read/write.
 *
 * Holds the loaded content + dirty edits for files opened in the code editor.
 * Keyed by "connectionId:path".
 */
import { create } from 'zustand';
import { createFileApi } from '../api/fileApi';

const api = createFileApi();

export type BufferState = 'loading' | 'ready' | 'saving' | 'error';

export interface FileBuffer {
  key: string;
  connectionId: string;
  path: string;
  original: string;
  draft: string;
  state: BufferState;
  error?: string;
}

export const bufferKey = (connectionId: string, path: string) => `${connectionId}:${path}`;

interface FileBufferStore {
  buffers: Record<string, FileBuffer>;

  open: (connectionId: string, path: string) => Promise<void>;
  edit: (key: string, draft: string) => void;
  save: (key: string) => Promise<void>;
  close: (key: string) => void;
}

export const useFileBufferStore = create<FileBufferStore>((set, get) => ({
  buffers: {},

  async open(connectionId, path) {
    const key = bufferKey(connectionId, path);
    const existing = get().buffers[key];
    if (existing && existing.state !== 'error') return;

    set((s) => ({
      buffers: {
        ...s.buffers,
        [key]: {
          key, connectionId, path,
          original: existing?.original ?? '',
          draft: existing?.draft ?? '',
          state: 'loading',
        },
      },
    }));

    try {
      const content = await api.readFile(connectionId, path);
      set((s) => {
        const cur = s.buffers[key];
        if (!cur) return s;
        return {
          buffers: {
            ...s.buffers,
            [key]: { ...cur, original: content, draft: content, state: 'ready', error: undefined },
          },
        };
      });
    } catch (e) {
      set((s) => {
        const cur = s.buffers[key];
        if (!cur) return s;
        return { buffers: { ...s.buffers, [key]: { ...cur, state: 'error', error: String(e) } } };
      });
    }
  },

  edit(key, draft) {
    set((s) => {
      const cur = s.buffers[key];
      if (!cur) return s;
      return { buffers: { ...s.buffers, [key]: { ...cur, draft } } };
    });
  },

  async save(key) {
    const cur = get().buffers[key];
    if (!cur || cur.state === 'saving') return;
    set((s) => ({ buffers: { ...s.buffers, [key]: { ...cur, state: 'saving' } } }));
    try {
      await api.writeFile(cur.connectionId, cur.path, cur.draft);
      set((s) => {
        const b = s.buffers[key];
        if (!b) return s;
        return {
          buffers: { ...s.buffers, [key]: { ...b, original: b.draft, state: 'ready', error: undefined } },
        };
      });
    } catch (e) {
      set((s) => {
        const b = s.buffers[key];
        if (!b) return s;
        return { buffers: { ...s.buffers, [key]: { ...b, state: 'error', error: String(e) } } };
      });
    }
  },

  close(key) {
    set((s) => {
      const buffers = { ...s.buffers };
      delete buffers[key];
      return { buffers };
    });
  },
}));
