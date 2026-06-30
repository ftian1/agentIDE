import { useState, useMemo, useEffect, lazy, Suspense } from 'react';
import { AppShell } from './components/layout/AppShell';
import { SecondarySidebar } from './components/layout/SecondarySidebar';
import { UpdateBanner } from './components/layout/UpdateBanner';
import { SplashScreen } from './components/layout/SplashScreen';
import { ExplorerPanel } from './components/explorer/ExplorerPanel';
import { ModelListPanel } from './components/models/ModelListPanel';
import { SessionManagerPanel } from './components/session/SessionManagerPanel';
import { SearchPanel } from './components/search/SearchPanel';
import { StatusPanel } from './components/status/StatusPanel';
import { MenuBar } from './components/menubar/MenuBar';
import { ConnectionBadge } from './components/status/ConnectionBadge';
import { ApprovalQueue } from './components/approval/ApprovalQueue';
import { CodeEditor } from './components/editor/CodeEditor';
import { AgentStdout } from './components/bottom/AgentStdout';
import { BottomTerminal } from './components/bottom/BottomTerminal';
import { McpLogs, FileSyncPanel, PortsPanel } from './components/bottom/BottomPanels';
import { HttpTrafficPanel } from './components/bottom/HttpTrafficPanel';
import { DebugView } from './components/debug/DebugView';
import { useLayoutStore } from './stores/layoutStore';
import { useWorkspaceView, useConnectionBootstrap } from './hooks/useWorkspaceView';
import { log, logViewSwitch } from './lib/debugLog';

/** Heavy components lazy-loaded — only fetched when actually needed. */
const CodeChangesSidebar = lazy(() =>
  import('./components/changes/CodeChangesSidebar').then(m => ({ default: m.CodeChangesSidebar })));
const CodeChangeEditor = lazy(() =>
  import('./components/changes/CodeChangeEditor').then(m => ({ default: m.CodeChangeEditor })));
// TerminalPane is NOT lazy — needed immediately when session spawns (race with relay events)
import { AgentColumnPanel } from './components/agentpanel/AgentColumnPanel';
import { AgentManagerPanel } from './components/agent/AgentManagerPanel';
const SessionDetail = lazy(() =>
  import('./components/detail/SessionDetail').then(m => ({ default: m.SessionDetail })));
const ConnectionDialog = lazy(() =>
  import('./components/connection/ConnectionDialog').then(m => ({ default: m.ConnectionDialog })));
const AgentBackendModal = lazy(() =>
  import('./components/settings/AgentBackendModal').then(m => ({ default: m.AgentBackendModal })));
const LlmProviderModal = lazy(() =>
  import('./components/settings/LlmProviderModal').then(m => ({ default: m.LlmProviderModal })));
const AgentEngineModal = lazy(() =>
  import('./components/settings/AgentEngineModal').then(m => ({ default: m.AgentEngineModal })));

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

function ToolsPanel() {
  return (
    <div className="flex flex-col h-full">
      <div className="px-3 py-3 border-b border-border">
        <span className="text-xs font-semibold text-text-secondary uppercase tracking-wider">
          Tools
        </span>
      </div>
      <div className="flex-1 flex items-center justify-center">
        <p className="text-xs text-text-secondary italic">Tools will appear here.</p>
      </div>
    </div>
  );
}

export function App() {
  const [showConnectionDialog, setShowConnectionDialog] = useState(false);
  const [splashVisible, setSplashVisible] = useState(true);
  const rightPanelVisible = useLayoutStore((s) => s.rightPanelVisible);
  const toggleRightPanel = useLayoutStore((s) => s.toggleRightPanel);
  const bottomPanelTab = useLayoutStore((s) => s.bottomPanelTab);
  const activeActivity = useLayoutStore((s) => s.activeActivity);
  const topBarVisible = useLayoutStore((s) => s.topBarVisible);
  const zenMode = useLayoutStore((s) => s.zenMode);
  const agentPanelVisible = useLayoutStore((s) => s.agentPanelVisible);
  const openModal = useLayoutStore((s) => s.openModal);
  const setOpenModal = useLayoutStore((s) => s.setOpenModal);
  const workspaceView = useWorkspaceView();

  // Load persisted connections from DB on startup
  useConnectionBootstrap();

  // Hide splash when the main UI is ready.
  useEffect(() => {
    const timer = setTimeout(() => setSplashVisible(false), 1800);
    return () => clearTimeout(timer);
  }, []);

  const sidebarContent = useMemo(() => {
    switch (activeActivity) {
      case 'explorer':
        return <ExplorerPanel />;
      case 'agentManager':
        return <AgentManagerPanel />;
      case 'sessionManager':
        return <SessionManagerPanel />;
      case 'search':
        return <SearchPanel />;
      case 'approvals':
        return <ApprovalQueue />;
      case 'tools':
        return <ToolsPanel />;
      case 'models':
        return <ModelListPanel />;
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
    // The bash terminal stays mounted across tab switches (hidden when inactive)
    // so its session scrollback survives. Other tabs render only when active.
    const other = (() => {
      switch (bottomPanelTab) {
        case 'agentStdout':
          return <AgentStdout />;
        case 'mcpLogs':
          return <McpLogs />;
        case 'fileSync':
          return <FileSyncPanel />;
        case 'ports':
          return <PortsPanel />;
        case 'httpTraffic':
          return <HttpTrafficPanel />;
        case 'problems':
          return (
            <div className="flex items-center justify-center h-full">
              <p className="text-xs text-text-secondary italic">No problems detected.</p>
            </div>
          );
        default:
          return null;
      }
    })();

    return (
      <div className="relative h-full">
        <div className={`absolute inset-0 ${bottomPanelTab === 'terminal' ? '' : 'hidden'}`}>
          <BottomTerminal />
        </div>
        {bottomPanelTab !== 'terminal' && <div className="absolute inset-0">{other}</div>}
      </div>
    );
  }, [bottomPanelTab]);

  const isDebugView = activeActivity === 'debug';
  const showChrome = !zenMode;

  // Log view switches for debugging
  useEffect(() => {
    logViewSwitch(isDebugView ? 'debug' : activeActivity);
  }, [isDebugView, activeActivity]);

  const mainContent = useMemo(() => {
    switch (workspaceView.kind) {
      case 'diff':
        return (
          <Suspense fallback={<Spinner label="Loading diff editor..." />}>
            <CodeChangeEditor />
          </Suspense>
        );
      case 'file':
        return (
          <CodeEditor connectionId={workspaceView.connectionId} path={workspaceView.path} />
        );
      case 'empty':
        return (
          <div className="flex-1 flex flex-col items-center justify-center">
            <div className="text-center space-y-4">
              <div className="text-5xl text-text-secondary opacity-20">▸</div>
              <div>
                <p className="text-text-secondary text-sm font-medium">Remote AI IDE</p>
                <p className="text-text-secondary text-xs mt-1">
                  Open a file from the Explorer, or review an agent patch from Source Control.
                </p>
              </div>
            </div>
          </div>
        );
    }
  }, [workspaceView, isDebugView]);

  return (
    <>
      <SplashScreen visible={splashVisible} />
      <UpdateBanner />
      <AppShell
        topBar={showChrome && topBarVisible ? <MenuBar rightSlot={<ConnectionBadge />} /> : undefined}
        sidebar={
          showChrome ? (
            <SecondarySidebar>
              <div className="flex flex-col h-full">
                <div className="flex-1 overflow-y-auto">{sidebarContent}</div>
                <ApprovalQueue />
              </div>
            </SecondarySidebar>
          ) : undefined
        }
        bottomPanelContent={bottomContent}
        agentPanel={
          showChrome && agentPanelVisible ? (
            <AppShell.AgentColumn>
              <AgentColumnPanel />
            </AppShell.AgentColumn>
          ) : undefined
        }
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
        overlay={
          isDebugView ? (
            <div className="absolute left-12 right-0 top-0 bottom-0 z-50 bg-bg-primary overflow-hidden">
              <DebugView />
            </div>
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

      {openModal === 'agentBackend' && (
        <Suspense fallback={null}>
          <AgentBackendModal onClose={() => setOpenModal(null)} />
        </Suspense>
      )}

      {openModal === 'llmProviders' && (
        <Suspense fallback={null}>
          <LlmProviderModal onClose={() => setOpenModal(null)} />
        </Suspense>
      )}

      {openModal === 'agentEngine' && (
        <Suspense fallback={null}>
          <AgentEngineModal onClose={() => setOpenModal(null)} />
        </Suspense>
      )}

    </>
  );
}
