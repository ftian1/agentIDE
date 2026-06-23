/**
 * Code Change Store — Zustand store for code change review state.
 *
 * Receives code:change and code:change_batch events from the backend.
 * Event listeners are set up externally (see init.ts) to avoid
 * side-effects inside the Zustand creator.
 */
import { create } from 'zustand';
import { detectLanguage } from '../lib/languageDetector';
import { computeDiff, reconstruct, type Hunk } from '../lib/diff';

export type ChangeSetStatus = 'pending' | 'complete';
export type FileChangeStatus = 'pending' | 'accepted' | 'rejected';
export type HunkDecision = 'pending' | 'accepted' | 'rejected';

export interface FileChange {
  id: string;
  changeSetId: string;
  sessionId: string;
  filePath: string;
  language: string;
  oldContent: string | null;
  newContent: string | null;
  diff: string;
  status: FileChangeStatus;
  /** Per-hunk accept/reject state, keyed by hunk id. */
  hunkDecisions: Record<string, HunkDecision>;
}

export interface ChangeSet {
  id: string;
  sessionId: string;
  description: string;
  status: ChangeSetStatus;
  files: Record<string, FileChange>;
  createdAt: string;
}

export interface CodeChangeStore {
  changeSets: Record<string, ChangeSet>;
  activeChangeSetId: string | null;

  _addChangeSet: (cs: ChangeSet) => void;
  _addChange: (change: FileChange) => void;
  _updateChangeStatus: (changeId: string, status: FileChangeStatus) => void;
  _updateChangeSetStatus: (changeSetId: string, status: ChangeSetStatus) => void;
  setActiveChangeSet: (id: string | null) => void;

  acceptChange: (changeId: string) => Promise<void>;
  rejectChange: (changeId: string) => Promise<void>;
  acceptAllInFile: (changeSetId: string, filePath: string) => Promise<void>;
  rejectAllInFile: (changeSetId: string, filePath: string) => Promise<void>;

  // Per-hunk staged-changes controls.
  setHunkDecision: (changeSetId: string, filePath: string, hunkId: string, decision: HunkDecision) => void;
  /** Apply currently-accepted hunks to the remote file via write_file. */
  applyAcceptedHunks: (changeSetId: string, filePath: string) => Promise<void>;
}

/** Hunks for a file change, recomputed from its old/new content. */
export function fileHunks(file: FileChange): Hunk[] {
  return computeDiff(file.oldContent ?? '', file.newContent ?? '').hunks;
}

export const useCodeChangeStore = create<CodeChangeStore>((set, get) => ({
  changeSets: {},
  activeChangeSetId: null,

  _addChangeSet: (cs) =>
    set((s) => ({
      changeSets: { ...s.changeSets, [cs.id]: cs },
    })),

  _addChange: (change) =>
    set((s) => {
      const cs = s.changeSets[change.changeSetId];
      if (!cs) return s;
      return {
        changeSets: {
          ...s.changeSets,
          [change.changeSetId]: {
            ...cs,
            files: { ...cs.files, [change.filePath]: change },
          },
        },
      };
    }),

  _updateChangeStatus: (changeId, status) =>
    set((s) => {
      for (const cs of Object.values(s.changeSets)) {
        for (const file of Object.values(cs.files)) {
          if (file.id === changeId) {
            return {
              changeSets: {
                ...s.changeSets,
                [cs.id]: {
                  ...cs,
                  files: {
                    ...cs.files,
                    [file.filePath]: { ...file, status },
                  },
                },
              },
            };
          }
        }
      }
      return s;
    }),

  _updateChangeSetStatus: (changeSetId, status) =>
    set((s) => {
      const cs = s.changeSets[changeSetId];
      if (!cs) return s;
      return {
        changeSets: {
          ...s.changeSets,
          [changeSetId]: { ...cs, status },
        },
      };
    }),

  setActiveChangeSet: (id) => set({ activeChangeSetId: id }),

  async acceptChange(_changeId: string) {
    // Will be wired when Tauri API is available
    get()._updateChangeStatus(_changeId, 'accepted');
  },

  async rejectChange(_changeId: string) {
    get()._updateChangeStatus(_changeId, 'rejected');
  },

  async acceptAllInFile(changeSetId: string, filePath: string) {
    const cs = get().changeSets[changeSetId];
    if (!cs) return;
    const file = cs.files[filePath];
    if (!file) return;
    get()._updateChangeStatus(file.id, 'accepted');
  },

  async rejectAllInFile(changeSetId: string, filePath: string) {
    const cs = get().changeSets[changeSetId];
    if (!cs) return;
    const file = cs.files[filePath];
    if (!file) return;
    get()._updateChangeStatus(file.id, 'rejected');
  },

  setHunkDecision(changeSetId, filePath, hunkId, decision) {
    set((s) => {
      const cs = s.changeSets[changeSetId];
      if (!cs) return s;
      const file = cs.files[filePath];
      if (!file) return s;
      return {
        changeSets: {
          ...s.changeSets,
          [changeSetId]: {
            ...cs,
            files: {
              ...cs.files,
              [filePath]: {
                ...file,
                hunkDecisions: { ...file.hunkDecisions, [hunkId]: decision },
              },
            },
          },
        },
      };
    });
  },

  async applyAcceptedHunks(changeSetId: string, filePath: string) {
    const cs = get().changeSets[changeSetId];
    if (!cs) return;
    const file = cs.files[filePath];
    if (!file) return;

    const hunks = fileHunks(file);
    const content = reconstruct(file.oldContent ?? '', hunks, file.hunkDecisions);

    // Resolve the connection for this session so we can write to the right host.
    const { useSessionStore } = await import('./sessionStore');
    const connectionId = useSessionStore.getState().sessions[file.sessionId]?.connectionId;
    if (!connectionId) {
      throw new Error(`No connection for session ${file.sessionId}`);
    }

    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('write_file', { connectionId, path: file.filePath, content });

    // Once written, the accepted hunks ARE the file: fold them into oldContent
    // so the diff view collapses them and any remaining pending hunks re-base
    // against the now-current file.
    const allDecided = hunks.every((h) => file.hunkDecisions[h.id] && file.hunkDecisions[h.id] !== 'pending');
    set((s) => {
      const cs2 = s.changeSets[changeSetId];
      if (!cs2) return s;
      const f2 = cs2.files[filePath];
      if (!f2) return s;
      return {
        changeSets: {
          ...s.changeSets,
          [changeSetId]: {
            ...cs2,
            files: {
              ...cs2.files,
              [filePath]: {
                ...f2,
                oldContent: content,
                status: allDecided ? 'accepted' : f2.status,
              },
            },
          },
        },
      };
    });
  },
}));

/**
 * Initialize code change event listeners.
 * Must be called once at app startup (not inside a React render).
 */
export function initCodeChangeListeners() {
  // Dynamic import to avoid bundling Tauri API in non-Tauri environments
  import('@tauri-apps/api/event').then(({ listen }) => {
    listen<{
      session_id: string; change_set_id: string; change_id: string;
      file_path: string; old_content: string | null; new_content: string | null; diff: string;
    }>('code:change', (event) => {
      const p = event.payload;
      const store = useCodeChangeStore.getState();

      if (!store.changeSets[p.change_set_id]) {
        store._addChangeSet({
          id: p.change_set_id,
          sessionId: p.session_id,
          description: `Changes from session ${p.session_id.slice(0, 8)}`,
          status: 'pending',
          files: {},
          createdAt: new Date().toISOString(),
        });
      }

      store._addChange({
        id: p.change_id,
        changeSetId: p.change_set_id,
        sessionId: p.session_id,
        filePath: p.file_path,
        language: detectLanguage(p.file_path),
        oldContent: p.old_content ?? null,
        newContent: p.new_content ?? null,
        diff: p.diff,
        status: 'pending',
        hunkDecisions: {},
      });
    });

    listen<{
      session_id: string; change_set_id: string;
      description: string; status: string; file_count: number;
    }>('code:change_batch', (event) => {
      const p = event.payload;
      const store = useCodeChangeStore.getState();

      if (store.changeSets[p.change_set_id]) {
        store._updateChangeSetStatus(p.change_set_id, p.status as ChangeSetStatus);
      } else {
        store._addChangeSet({
          id: p.change_set_id,
          sessionId: p.session_id,
          description: p.description,
          status: p.status as ChangeSetStatus,
          files: {},
          createdAt: new Date().toISOString(),
        });
      }
    });
  }).catch(() => {
    // Tauri API not available (e.g., running in browser dev mode)
  });
}
