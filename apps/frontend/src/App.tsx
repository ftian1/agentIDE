import { useState, useMemo } from 'react';
import { AppShell } from './components/layout/AppShell';
import { SecondarySidebar } from './components/layout/SecondarySidebar';
import { ConnectionDialog } from './components/connection/ConnectionDialog';
import { ExplorerPanel } from './components/explorer/ExplorerPanel';
import { SearchPanel } from './components/search/SearchPanel';
import { CodeChangesSidebar } from './components/changes/CodeChangesSidebar';
import { CodeChangeEditor } from './components/changes/CodeChangeEditor';
import { TerminalPane } from './components/terminal/TerminalPane';
import { SessionDetail } from './components/detail/SessionDetail';
import { StatusPanel } from './components/status/StatusPanel';
import { useLayoutStore } from './stores/layoutStore';

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

  // Sidebar content switches based on ActivityBar selection
  const sidebarContent = useMemo(() => {
    switch (activeActivity) {
      case 'explorer':
        return <ExplorerPanel onNewConnection={() => setShowConnectionDialog(true)} />;
      case 'search':
        return <SearchPanel />;
      case 'sourceControl':
        return <CodeChangesSidebar />;
      case 'settings':
        return <SettingsPanel />;
      default:
        return null;
    }
  }, [activeActivity]);

  // Bottom panel content switches based on panel tab
  const bottomContent = useMemo(() => {
    switch (bottomPanelTab) {
      case 'terminal':
        return <TerminalPane />;
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

  // Main content: show CodeChangeEditor if a code change file is open, otherwise welcome screen
  const mainContent = useMemo(() => {
    const activeTab = editorTabs.find((t) => t.id === activeEditorTabId);
    if (activeTab?.changeSetId) {
      return <CodeChangeEditor />;
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
              <SessionDetail />
            </AppShell.RightPanel>
          ) : undefined
        }
      >
        {mainContent}
      </AppShell>

      {/* Modal overlay */}
      {showConnectionDialog && (
        <ConnectionDialog onClose={() => setShowConnectionDialog(false)} />
      )}
    </>
  );
}
