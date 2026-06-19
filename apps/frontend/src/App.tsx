import { useState } from 'react';
import { AppShell } from './components/layout/AppShell';
import { SecondarySidebar } from './components/layout/SecondarySidebar';
import { ConnectionDialog } from './components/connection/ConnectionDialog';
import { SessionRail } from './components/session/SessionRail';
import { TerminalPane } from './components/terminal/TerminalPane';
import { SessionDetail } from './components/detail/SessionDetail';
import { StatusPanel } from './components/status/StatusPanel';
import { useLayoutStore } from './stores/layoutStore';

export function App() {
  const [showConnectionDialog, setShowConnectionDialog] = useState(false);
  const rightPanelVisible = useLayoutStore((s) => s.rightPanelVisible);
  const toggleRightPanel = useLayoutStore((s) => s.toggleRightPanel);
  const bottomPanelTab = useLayoutStore((s) => s.bottomPanelTab);

  return (
    <>
      <AppShell
        sidebar={
          <SecondarySidebar>
            <SessionRail onNewConnection={() => setShowConnectionDialog(true)} />
          </SecondarySidebar>
        }
        bottomPanelContent={
          bottomPanelTab === 'terminal' ? <TerminalPane /> : null
        }
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
        {/* Main content: editor area (empty for now; Phase 4 adds Monaco diff editor) */}
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
      </AppShell>

      {/* Modal overlay */}
      {showConnectionDialog && (
        <ConnectionDialog onClose={() => setShowConnectionDialog(false)} />
      )}
    </>
  );
}
