/**
 * Layout Store — Zustand store for IDE chrome layout state.
 *
 * Drives: ActivityBar, SecondarySidebar, EditorTabBar, BottomPanel, RightPanel.
 */
import { create } from 'zustand';

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

export const useLayoutStore = create<LayoutStore>((set) => ({
  activeActivity: 'explorer',
  setActiveActivity: (id) => set({ activeActivity: id }),

  secondarySidebarVisible: true,
  secondarySidebarWidth: 260,
  toggleSecondarySidebar: () =>
    set((s) => ({ secondarySidebarVisible: !s.secondarySidebarVisible })),
  setSecondarySidebarWidth: (w) => set({ secondarySidebarWidth: Math.max(200, Math.min(400, w)) }),

  bottomPanelVisible: true,
  bottomPanelHeight: 220,
  bottomPanelTab: 'terminal',
  setBottomPanelTab: (tab) => set({ bottomPanelTab: tab, bottomPanelVisible: true }),
  toggleBottomPanel: () =>
    set((s) => ({ bottomPanelVisible: !s.bottomPanelVisible })),
  setBottomPanelHeight: (h) => set({ bottomPanelHeight: Math.max(100, Math.min(600, h)) }),

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
