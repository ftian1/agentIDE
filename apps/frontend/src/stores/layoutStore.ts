/**
 * Layout Store — Zustand store for IDE chrome layout state.
 *
 * Drives: ActivityBar, SecondarySidebar, EditorTabBar, BottomPanel, RightPanel.
 * Persisted to SQLite (primary) + localStorage (fallback).
 */
import { create } from 'zustand';
import { savePersisted } from '../lib/storage';

const LAYOUT_KEY = 'remote-ai-ide:layout';

const PERSIST_KEYS: (keyof LayoutStore)[] = [
  'secondarySidebarWidth', 'bottomPanelHeight', 'bottomPanelVisible',
  'secondarySidebarVisible', 'bottomPanelTab', 'topBarVisible',
  'agentPanelVisible', 'agentColumnWidth',
];

function persistLayout(state: Partial<LayoutStore>) {
  const toSave: Record<string, unknown> = {};
  for (const key of PERSIST_KEYS) {
    if (state[key] !== undefined) toSave[key] = state[key];
  }
  savePersisted(LAYOUT_KEY, toSave);
}

export type ActivityId = 'agentManager' | 'explorer' | 'sessionManager' | 'search' | 'approvals' | 'tools' | 'sourceControl' | 'models' | 'debug' | 'settings';
export type BottomPanelTab = 'terminal' | 'agentStdout' | 'mcpLogs' | 'fileSync' | 'problems' | 'ports' | 'httpTraffic';
export type ModalId = 'agentBackend' | 'llmProviders' | 'agentEngine';

export interface EditorTab {
  id: string;
  filePath: string;
  label: string;
  icon?: 'add' | 'modify' | 'delete';
  changeSetId?: string;
  /** When set, this tab opens a remote file in the editable code editor. */
  connectionId?: string;
}

export interface LayoutStore {
  _init: () => Promise<void>;

  // Activity bar
  activeActivity: ActivityId;
  setActiveActivity: (id: ActivityId) => void;

  // Top menu bar
  topBarVisible: boolean;
  toggleTopBar: () => void;

  // Zen mode (hide all chrome)
  zenMode: boolean;
  toggleZenMode: () => void;

  // Modal overlay
  openModal: ModalId | null;
  setOpenModal: (id: ModalId | null) => void;

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

  // Bottom panel — dedicated bash session
  bottomPanelSessionId: string | null;
  setBottomPanelSessionId: (id: string | null) => void;

  // Right panel (session detail)
  rightPanelVisible: boolean;
  setRightPanelVisible: (v: boolean) => void;
  toggleRightPanel: () => void;

  // Agent panel (right-side Claude Code conversation)
  agentPanelVisible: boolean;
  toggleAgentPanel: () => void;
  agentColumnWidth: number;
  setAgentColumnWidth: (w: number) => void;

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

  topBarVisible: true,
  toggleTopBar: () =>
    set((s) => {
      const v = !s.topBarVisible;
      persistLayout({ topBarVisible: v });
      return { topBarVisible: v };
    }),

  zenMode: false,
  toggleZenMode: () => set((s) => ({ zenMode: !s.zenMode })),

  openModal: null,
  setOpenModal: (id) => set({ openModal: id }),

  secondarySidebarVisible: true,
  secondarySidebarWidth: 260,
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

  bottomPanelVisible: true,
  bottomPanelHeight: 220,
  bottomPanelTab: 'terminal',
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

  bottomPanelSessionId: null,
  setBottomPanelSessionId: (id) => set({ bottomPanelSessionId: id }),

  rightPanelVisible: false,
  setRightPanelVisible: (v) => set({ rightPanelVisible: v }),
  toggleRightPanel: () =>
    set((s) => ({ rightPanelVisible: !s.rightPanelVisible })),

  agentPanelVisible: true,
  toggleAgentPanel: () =>
    set((s) => {
      const v = !s.agentPanelVisible;
      persistLayout({ agentPanelVisible: v });
      return { agentPanelVisible: v };
    }),
  agentColumnWidth: 720,
  setAgentColumnWidth: (w) => {
    // Lower bound ~660px so the native agent terminal keeps ≥80 columns at the
    // 14px monospace font (≈8.4px/col + padding) — below that the CLI TUI wraps
    // and renders garbled ("错行"). Upper bound leaves room for the editor.
    const clamped = Math.max(660, Math.min(1100, w));
    persistLayout({ agentColumnWidth: clamped });
    set({ agentColumnWidth: clamped });
  },

  _init: async () => {
    const { loadPersisted } = await import('../lib/storage');
    const saved = await loadPersisted<Partial<LayoutStore>>(LAYOUT_KEY, {});
    if (saved && Object.keys(saved).length > 0) {
      useLayoutStore.setState((s) => ({ ...s, ...saved }));
    }
  },

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
