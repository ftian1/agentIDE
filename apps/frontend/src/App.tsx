import { useState } from 'react';
import { AppShell } from './components/layout/AppShell';
import { ConnectionDialog } from './components/connection/ConnectionDialog';
import { SessionRail } from './components/session/SessionRail';
import { TerminalPane } from './components/terminal/TerminalPane';
import { SessionDetail } from './components/detail/SessionDetail';
import { StatusPanel } from './components/status/StatusPanel';

export function App() {
  const [showConnectionDialog, setShowConnectionDialog] = useState(false);
  const [showDetail, setShowDetail] = useState(false);

  return (
    <AppShell>
      {/* Left sidebar: session list */}
      <AppShell.Sidebar>
        <SessionRail onNewConnection={() => setShowConnectionDialog(true)} />
      </AppShell.Sidebar>

      {/* Center: terminal */}
      <AppShell.Main>
        <TerminalPane />
      </AppShell.Main>

      {/* Right: session detail dock (togglable) */}
      {showDetail && (
        <AppShell.RightPanel onClose={() => setShowDetail(false)}>
          <SessionDetail />
        </AppShell.RightPanel>
      )}

      {/* Bottom: status bar */}
      <AppShell.StatusBar>
        <StatusPanel onToggleDetail={() => setShowDetail((v) => !v)} />
      </AppShell.StatusBar>

      {/* Connection dialog (modal) */}
      {showConnectionDialog && (
        <ConnectionDialog onClose={() => setShowConnectionDialog(false)} />
      )}
    </AppShell>
  );
}
