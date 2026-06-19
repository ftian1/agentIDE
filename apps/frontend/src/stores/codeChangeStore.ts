/**
 * Code Change Store — Zustand store for code change review state.
 *
 * Listens for Tauri `code:change` and `code:change_batch` events from the
 * backend and manages change sets, file changes, and accept/reject actions.
 */
import { create } from 'zustand';
import { createTerminalApi } from '../api/terminalApi';
import { detectLanguage } from '../lib/languageDetector';

const api = createTerminalApi();

export type ChangeSetStatus = 'pending' | 'complete';
export type FileChangeStatus = 'pending' | 'accepted' | 'rejected';

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
}

export const useCodeChangeStore = create<CodeChangeStore>((set, get) => {
  // Listen for code:change events from the backend
  api.onCodeChange((event) => {
    const store = get();
    const csId = event.change_set_id;

    // Create or update the change set
    if (!store.changeSets[csId]) {
      store._addChangeSet({
        id: csId,
        sessionId: event.session_id,
        description: `Changes from session ${event.session_id.slice(0, 8)}`,
        status: 'pending',
        files: {},
        createdAt: new Date().toISOString(),
      });
    }

    store._addChange({
      id: event.change_id,
      changeSetId: csId,
      sessionId: event.session_id,
      filePath: event.file_path,
      language: detectLanguage(event.file_path),
      oldContent: event.old_content ?? null,
      newContent: event.new_content ?? null,
      diff: event.diff,
      status: 'pending',
    });
  });

  // Listen for batch events
  api.onCodeChangeBatch((event) => {
    const store = get();
    const csId = event.change_set_id;

    if (store.changeSets[csId]) {
      store._updateChangeSetStatus(csId, event.status as ChangeSetStatus);
    } else {
      store._addChangeSet({
        id: csId,
        sessionId: event.session_id,
        description: event.description,
        status: event.status as ChangeSetStatus,
        files: {},
        createdAt: new Date().toISOString(),
      });
    }
  });

  return {
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

    async acceptChange(changeId: string) {
      const store = get();
      // Find the file change
      for (const cs of Object.values(store.changeSets)) {
        for (const file of Object.values(cs.files)) {
          if (file.id === changeId && file.newContent !== null) {
            await api.applyChange(file.sessionId, file.filePath, file.newContent);
            store._updateChangeStatus(changeId, 'accepted');
            return;
          }
        }
      }
    },

    async rejectChange(changeId: string) {
      await api.rejectChange(changeId);
      get()._updateChangeStatus(changeId, 'rejected');
    },

    async acceptAllInFile(changeSetId: string, filePath: string) {
      const cs = get().changeSets[changeSetId];
      if (!cs) return;
      const file = cs.files[filePath];
      if (!file || file.newContent === null) return;
      await api.applyChange(file.sessionId, file.filePath, file.newContent);
      get()._updateChangeStatus(file.id, 'accepted');
    },

    async rejectAllInFile(changeSetId: string, filePath: string) {
      const cs = get().changeSets[changeSetId];
      if (!cs) return;
      const file = cs.files[filePath];
      if (!file) return;
      await api.rejectChange(file.id);
      get()._updateChangeStatus(file.id, 'rejected');
    },
  };
});
