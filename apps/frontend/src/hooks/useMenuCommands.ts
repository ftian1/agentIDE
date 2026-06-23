/**
 * useMenuCommands — headless hook exposing top-menu-bar command actions.
 *
 * Resilient to layoutStore toggles that may not exist yet (defensive optional
 * chaining), so it typechecks independently of layout-store extensions.
 */
import { useLayoutStore } from '../stores/layoutStore';

export interface MenuCommands {
  // File / Remote
  openRemoteProject: () => void;
  openConnectionManager: () => void;
  syncRemoteFs: () => void;
  // Agent Engine
  openAgentBackendSettings: () => void;
  openModelRoute: () => void;
  // Git & Review
  reviewCommits: () => void;
  undoLastSession: () => void;
  // View
  toggleFileExplorer: () => void;
  toggleAgentPanel: () => void;
  toggleTerminalDock: () => void;
  splitEditorRight: () => void;
  zenMode: () => void;
  // Help
  openDocs: () => void;
  openShortcuts: () => void;
  openReleaseNotes: () => void;
  openAbout: () => void;
}

export function useMenuCommands(): MenuCommands {
  const setActiveActivity = useLayoutStore((s) => s.setActiveActivity);

  // Some toggles/modal setters are added by later layoutStore extensions.
  // Access them defensively so this hook compiles standalone.
  const ls = () => useLayoutStore.getState() as any;

  return {
    openRemoteProject: () => setActiveActivity('agentManager'),
    openConnectionManager: () => setActiveActivity('agentManager'),
    syncRemoteFs: () => {
      // TODO: wire up remote filesystem sync command
    },

    openAgentBackendSettings: () => ls().setOpenModal?.('agentBackend'),
    openModelRoute: () => {
      // TODO: wire up model route override modal
    },

    reviewCommits: () => setActiveActivity('sourceControl'),
    undoLastSession: () => {
      // TODO: wire up undo-last-agent-session
    },

    toggleFileExplorer: () => ls().toggleSecondarySidebar?.(),
    toggleAgentPanel: () => ls().toggleAgentPanel?.(),
    toggleTerminalDock: () => ls().toggleBottomPanel?.(),
    splitEditorRight: () => {
      // TODO: wire up split-editor-right
    },
    zenMode: () => ls().toggleZenMode?.(),

    openDocs: () => {
      // TODO: open documentation
    },
    openShortcuts: () => {
      // TODO: open keyboard shortcuts
    },
    openReleaseNotes: () => {
      // TODO: open release notes
    },
    openAbout: () => {
      // TODO: open about dialog
    },
  };
}
