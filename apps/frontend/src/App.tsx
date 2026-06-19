import { useState, useMemo, lazy, Suspense } from 'react';
import { AppShell } from './components/layout/AppShell';
import { SecondarySidebar } from './components/layout/SecondarySidebar';
import { ExplorerPanel } from './components/explorer/ExplorerPanel';
import { SearchPanel } from './components/search/SearchPanel';
import { StatusPanel } from './components/status/StatusPanel';
import { useLayoutStore } from './stores/layoutStore';

/** Heavy components lazy-loaded — only fetched when actually needed. */
const CodeChangesSidebar = lazy(() =>
  import('./components/changes/CodeChangesSidebar').then(m => ({ default: m.CodeChangesSidebar })));
const CodeChangeEditor = lazy(() =>
  import('./components/changes/CodeChangeEditor').then(m => ({ default: m.CodeChangeEditor })));
const TerminalPane = lazy(() =>
  import('./components/terminal/TerminalPane').then(m => ({ default: m.TerminalPane })));
const SessionDetail = lazy(() =>
  import('./components/detail/SessionDetail').then(m => ({ default: m.SessionDetail })));
const ConnectionDialog = lazy(() =>
  import('./components/connection/ConnectionDialog').then(m => ({ default: m.ConnectionDialog })));

/** Minimal loading placeholder — avoids layout shift. */
function Spinner({ label }: { label?: string }) {
  return (
    <div className="flex items-center justify-center h-full">
      <div className="text-center space-y-2">
        <div className="w-5 h-5 border-2 border-accent/30 border-t-accent rounded-full animate-spin mx-auto" />
        {label && <p className="text-xs text-text-secondary">{label}</p>}
      </div>
    </div>
  );
}

function SettingsPanel() {
  return (
    <div className="flex flex-col h-full">
      <div className="px-3 py-3 border-b border-border">
        <span className="text-xs font-semibold text-text-secondary uppercase tracking-wider">
          Settings
        </span>
      </div>
      <div className="flex-1 flex items-center justify-center">
        <p className="text-xs text-text-secondary italic">Settings will appear here.</p>
      </div>
    </div>
  );
}

export function App() {
  const [showConnectionDialog, setShowConnectionDialog] = useState(false);
  const rightPanelVisible = useLayoutStore((s) => s.rightPanelVisible);
  const toggleRightPanel = useLayoutStore((s) => s.toggleRightPanel);
  const bottomPanelTab = useLayoutStore((s) => s.bottomPanelTab);
  const activeActivity = useLayoutStore((s) => s.activeActivity);
  const editorTabs = useLayoutStore((s) => s.editorTabs);
  const activeEditorTabId = useLayoutStore((s) => s.activeEditorTabId);

  const sidebarContent = useMemo(() => {
    switch (activeActivity) {
      case 'explorer':
        return <ExplorerPanel onNewConnection={() => setShowConnectionDialog(true)} />;
      case 'search':
        return <SearchPanel />;
      case 'sourceControl':
        return (
          <Suspense fallback={<Spinner label="Loading changes..." />}>
            <CodeChangesSidebar />
          </Suspense>
        );
      case 'settings':
        return <SettingsPanel />;
      default:
        return null;
    }
  }, [activeActivity]);

  const bottomContent = useMemo(() => {
    switch (bottomPanelTab) {
      case 'terminal':
        return (
          <Suspense fallback={<Spinner label="Loading terminal..." />}>
            <TerminalPane />
          </Suspense>
        );
      case 'problems':
        return (
          <div className="flex items-center justify-center h-full">
            <p className="text-xs text-text-secondary italic">No problems detected.</p>
          </div>
        );
      case 'output':
        return (
          <div className="flex items-center justify-center h-full">
            <p className="text-xs text-text-secondary italic">Output will appear here.</p>
          </div>
        );
      case 'codeChanges':
        return (
          <div className="flex items-center justify-center h-full">
            <p className="text-xs text-text-secondary italic">
              Code changes summary — open files from Source Control to review diffs.
            </p>
          </div>
        );
      default:
        return null;
    }
  }, [bottomPanelTab]);

  const mainContent = useMemo(() => {
    const activeTab = editorTabs.find((t) => t.id === activeEditorTabId);
    if (activeTab?.changeSetId) {
      return (
        <Suspense fallback={<Spinner label="Loading diff editor..." />}>
          <CodeChangeEditor />
        </Suspense>
      );
    }
    return (
      <div className="flex-1 flex flex-col items-center justify-center">
        <div className="text-center space-y-4">
          <div className="text-5xl text-text-secondary opacity-20">▸</div>
          <div>
            <p className="text-text-secondary text-sm font-medium">Remote AI IDE</p>
            <p className="text-text-secondary text-xs mt-1">
              Spawn a session or connect to a remote host to get started.
            </p>
          </div>
        </div>
      </div>
    );
  }, [editorTabs, activeEditorTabId]);

  return (
    <>
      <AppShell
        sidebar={
          <SecondarySidebar>
            {sidebarContent}
          </SecondarySidebar>
        }
        bottomPanelContent={bottomContent}
        statusBar={
          <AppShell.StatusBar>
            <StatusPanel onToggleDetail={toggleRightPanel} />
          </AppShell.StatusBar>
        }
        rightPanel={
          rightPanelVisible ? (
            <AppShell.RightPanel onClose={toggleRightPanel}>
              <Suspense fallback={<Spinner label="Loading..." />}>
                <SessionDetail />
              </Suspense>
            </AppShell.RightPanel>
          ) : undefined
        }
      >
        {mainContent}
      </AppShell>

      {showConnectionDialog && (
        <Suspense fallback={null}>
          <ConnectionDialog onClose={() => setShowConnectionDialog(false)} />
        </Suspense>
      )}
    </>
  );
}
