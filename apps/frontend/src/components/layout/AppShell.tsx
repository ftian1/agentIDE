import type { ReactNode } from 'react';
import { useRef, useCallback, useEffect } from 'react';
import { ActivityBar } from '../activity/ActivityBar';
import { SecondarySidebar } from './SecondarySidebar';
import { EditorTabBar } from './EditorTabBar';
import { BottomPanel } from './BottomPanel';
import { useLayoutStore } from '../../stores/layoutStore';

interface AppShellProps {
  children: ReactNode;
  topBar?: ReactNode;
  sidebar?: ReactNode;
  bottomPanelContent?: ReactNode;
  statusBar?: ReactNode;
  rightPanel?: ReactNode;
  agentPanel?: ReactNode;
  /** Full-screen overlay rendered inside the main row (right of ActivityBar). */
  overlay?: ReactNode;
}

/**
 * Antigravity-style IDE layout:
 *
 *   TopBar (menu + search + status)
 *   ActivityBar | SecondarySidebar | EditorTabBar + ContentArea + BottomPanel | AgentPanel | RightPanel
 *   StatusBar (bottom)
 */
export function AppShell({
  children,
  topBar,
  sidebar,
  bottomPanelContent,
  statusBar,
  rightPanel,
  agentPanel,
  overlay,
}: AppShellProps) {
  return (
    <div className="flex flex-col h-screen bg-bg-primary">
      {/* Top menu bar */}
      {topBar}

      {/* Main row */}
      <div className="flex flex-1 overflow-hidden relative">
        {/* Activity Bar (leftmost) */}
        <ActivityBar />

        {/* Secondary Sidebar (explorer / search / source control / settings) */}
        {sidebar}

        {/* Center column: editor tabs + content + bottom panel */}
        <div className="flex-1 flex flex-col overflow-hidden">
          <EditorTabBar />
          {/* Content area (main editor workspace or empty state) */}
          <div className="flex-1 overflow-hidden">
            {children}
          </div>
          {/* Bottom panel (terminal / problems / output / code changes) */}
          <BottomPanel>
            {bottomPanelContent}
          </BottomPanel>
        </div>

        {/* Agent panel (right-side Claude Code conversation) */}
        {agentPanel}

        {/* Right panel (session detail) */}
        {rightPanel}

        {/* Overlay (e.g. DebugView) — covers everything right of ActivityBar */}
        {overlay}
      </div>

      {/* Status Bar */}
      {statusBar}
    </div>
  );
}

/* ── Sub-components for backward compatibility ── */

function Sidebar({ children }: { children: ReactNode }) {
  return (
    <aside className="w-64 flex-shrink-0 bg-bg-secondary border-r border-border flex flex-col">
      {children}
    </aside>
  );
}

function Main({ children }: { children: ReactNode }) {
  return (
    <main className="flex-1 flex flex-col overflow-hidden bg-bg-primary">
      {children}
    </main>
  );
}

function RightPanel({ children, onClose }: { children: ReactNode; onClose: () => void }) {
  return (
    <aside className="w-80 flex-shrink-0 bg-bg-secondary border-l border-border flex flex-col">
      <div className="flex items-center justify-between px-3 py-2 border-b border-border">
        <span className="text-xs font-semibold text-text-secondary uppercase tracking-wider">
          Session Detail
        </span>
        <button
          onClick={onClose}
          className="text-text-secondary hover:text-text-primary text-sm px-1"
        >
          ✕
        </button>
      </div>
      <div className="flex-1 overflow-y-auto">{children}</div>
    </aside>
  );
}

function StatusBar({ children }: { children: ReactNode }) {
  return (
    <footer className="h-6 flex-shrink-0 bg-bg-secondary border-t border-border flex items-center px-3">
      {children}
    </footer>
  );
}

function AgentColumn({ children }: { children: ReactNode }) {
  const width = useLayoutStore((s) => s.agentColumnWidth);
  const setWidth = useLayoutStore((s) => s.setAgentColumnWidth);
  const resizing = useRef(false);

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    resizing.current = true;
  }, []);

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      if (!resizing.current) return;
      // Right-docked column: width grows as the cursor moves left.
      setWidth(window.innerWidth - e.clientX);
    };
    const onUp = () => { resizing.current = false; };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    return () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
  }, [setWidth]);

  return (
    <aside
      className="relative flex-shrink-0 bg-bg-secondary border-l border-border flex flex-col"
      style={{ width }}
    >
      {/* Resize handle (left edge) */}
      <div
        onMouseDown={onMouseDown}
        className="absolute left-0 top-0 bottom-0 w-1.5 -ml-0.5 cursor-col-resize hover:bg-accent/30 transition-colors z-10"
      />
      {children}
    </aside>
  );
}

AppShell.Sidebar = Sidebar;
AppShell.Main = Main;
AppShell.RightPanel = RightPanel;
AppShell.StatusBar = StatusBar;
AppShell.AgentColumn = AgentColumn;
