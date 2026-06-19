/**
 * Layout Store — Zustand store for IDE chrome layout state.
 *
 * Drives: ActivityBar, SecondarySidebar, EditorTabBar, BottomPanel, RightPanel.
 */
import { create } from 'zustand';

const LAYOUT_KEY = 'remote-ai-ide:layout';

function loadPersisted(): Partial<LayoutStore> {
  try {
    const raw = localStorage.getItem(LAYOUT_KEY);
    if (raw) return JSON.parse(raw);
  } catch { /* ignore */ }
  return {};
}

function persistLayout(state: Partial<LayoutStore>) {
  try {
    const toSave: Record<string, unknown> = {};
    for (const key of [
      'secondarySidebarWidth',
      'bottomPanelHeight',
      'bottomPanelVisible',
      'secondarySidebarVisible',
      'bottomPanelTab',
    ]) {
      const k = key as keyof LayoutStore;
      if (state[k] !== undefined) toSave[key] = state[k];
    }
    localStorage.setItem(LAYOUT_KEY, JSON.stringify(toSave));
  } catch { /* ignore */ }
}

export type ActivityId = 'explorer' | 'search' | 'sourceControl' | 'settings';
export type BottomPanelTab = 'terminal' | 'problems' | 'output' | 'codeChanges';

export interface EditorTab {
  id: string;
  filePath: string;
  label: string;
  icon?: 'add' | 'modify' | 'delete';
  changeSetId?: string;
}

export interface LayoutStore {
  // Activity bar
  activeActivity: ActivityId;
  setActiveActivity: (id: ActivityId) => void;

  // Secondary sidebar
  secondarySidebarVisible: boolean;
  secondarySidebarWidth: number;
  toggleSecondarySidebar: () => void;
  setSecondarySidebarWidth: (w: number) => void;

  // Bottom panel
  bottomPanelVisible: boolean;
  bottomPanelHeight: number;
  bottomPanelTab: BottomPanelTab;
  setBottomPanelTab: (tab: BottomPanelTab) => void;
  toggleBottomPanel: () => void;
  setBottomPanelHeight: (h: number) => void;

  // Right panel
  rightPanelVisible: boolean;
  setRightPanelVisible: (v: boolean) => void;
  toggleRightPanel: () => void;

  // Editor tabs
  editorTabs: EditorTab[];
  activeEditorTabId: string | null;
  addEditorTab: (tab: EditorTab) => void;
  removeEditorTab: (id: string) => void;
  setActiveEditorTab: (id: string | null) => void;
}

const persisted = loadPersisted();

export const useLayoutStore = create<LayoutStore>((set) => ({
  activeActivity: 'explorer',
  setActiveActivity: (id) => set({ activeActivity: id }),

  secondarySidebarVisible: persisted.secondarySidebarVisible ?? true,
  secondarySidebarWidth: persisted.secondarySidebarWidth ?? 260,
  toggleSecondarySidebar: () =>
    set((s) => {
      const v = !s.secondarySidebarVisible;
      persistLayout({ secondarySidebarVisible: v });
      return { secondarySidebarVisible: v };
    }),
  setSecondarySidebarWidth: (w) => {
    const clamped = Math.max(200, Math.min(400, w));
    persistLayout({ secondarySidebarWidth: clamped });
    set({ secondarySidebarWidth: clamped });
  },

  bottomPanelVisible: persisted.bottomPanelVisible ?? true,
  bottomPanelHeight: persisted.bottomPanelHeight ?? 220,
  bottomPanelTab: (persisted.bottomPanelTab as BottomPanelTab) ?? 'terminal',
  setBottomPanelTab: (tab) => {
    persistLayout({ bottomPanelTab: tab });
    set({ bottomPanelTab: tab, bottomPanelVisible: true });
  },
  toggleBottomPanel: () =>
    set((s) => {
      const v = !s.bottomPanelVisible;
      persistLayout({ bottomPanelVisible: v });
      return { bottomPanelVisible: v };
    }),
  setBottomPanelHeight: (h) => {
    const clamped = Math.max(100, Math.min(600, h));
    persistLayout({ bottomPanelHeight: clamped });
    set({ bottomPanelHeight: clamped });
  },

  rightPanelVisible: false,
  setRightPanelVisible: (v) => set({ rightPanelVisible: v }),
  toggleRightPanel: () =>
    set((s) => ({ rightPanelVisible: !s.rightPanelVisible })),

  editorTabs: [],
  activeEditorTabId: null,
  addEditorTab: (tab) =>
    set((s) => {
      const exists = s.editorTabs.find((t) => t.id === tab.id);
      if (exists) return { activeEditorTabId: tab.id };
      return {
        editorTabs: [...s.editorTabs, tab],
        activeEditorTabId: tab.id,
      };
    }),
  removeEditorTab: (id) =>
    set((s) => {
      const next = s.editorTabs.filter((t) => t.id !== id);
      return {
        editorTabs: next,
        activeEditorTabId:
          s.activeEditorTabId === id
            ? next.length > 0
              ? next[next.length - 1].id
              : null
            : s.activeEditorTabId,
      };
    }),
  setActiveEditorTab: (id) => set({ activeEditorTabId: id }),
}));
