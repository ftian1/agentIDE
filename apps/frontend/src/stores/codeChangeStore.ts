/**
 * Code Change Store — Zustand store for code change review state.
 *
 * Manages change sets, file changes, and accept/reject actions.
 * Changes arrive from the backend via Tauri `code:change` events.
 */
import { create } from 'zustand';

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
    // Will be wired to Tauri IPC in Phase 5
    get()._updateChangeStatus(_changeId, 'accepted');
  },

  async rejectChange(_changeId: string) {
    get()._updateChangeStatus(_changeId, 'rejected');
  },

  async acceptAllInFile(changeSetId: string, filePath: string) {
    const cs = get().changeSets[changeSetId];
    if (!cs) return;
    for (const file of Object.values(cs.files)) {
      if (file.filePath === filePath) {
        get()._updateChangeStatus(file.id, 'accepted');
      }
    }
  },

  async rejectAllInFile(changeSetId: string, filePath: string) {
    const cs = get().changeSets[changeSetId];
    if (!cs) return;
    for (const file of Object.values(cs.files)) {
      if (file.filePath === filePath) {
        get()._updateChangeStatus(file.id, 'rejected');
      }
    }
  },
}));
