/**
 * Workspace view-model (layer 2) — headless hooks that wrap business-store
 * reads so layout/presentation components never touch layer-1 stores directly.
 *
 * See design.md §4.13 (UI Layout 与底层代码分离).
 */
import { useEffect } from 'react';
import { useLayoutStore } from '../stores/layoutStore';
import { useConnectionStore } from '../stores/connectionStore';

/** What the center editor area should render, as semantic state (no JSX). */
export type WorkspaceView =
  | { kind: 'diff' }
  | { kind: 'file'; connectionId: string; path: string }
  | { kind: 'empty' };

/**
 * Derives the center-area view from editor tabs.
 * The center column is editor-only (file buffers + patch-preview diffs); the
 * agent terminal now lives in the right column, so terminal is not a center
 * view. Components map this to a component; they don't reach into stores.
 */
export function useWorkspaceView(): WorkspaceView {
  const editorTabs = useLayoutStore((s) => s.editorTabs);
  const activeEditorTabId = useLayoutStore((s) => s.activeEditorTabId);

  const activeTab = editorTabs.find((t) => t.id === activeEditorTabId);
  if (activeTab?.changeSetId) return { kind: 'diff' };
  if (activeTab?.connectionId) {
    return { kind: 'file', connectionId: activeTab.connectionId, path: activeTab.filePath };
  }

  return { kind: 'empty' };
}

/** Loads persisted connections once on mount. Keeps the effect out of layout code. */
export function useConnectionBootstrap(): void {
  const loadConnections = useConnectionStore((s) => s.loadConnections);
  useEffect(() => {
    loadConnections();
  }, [loadConnections]);
}
