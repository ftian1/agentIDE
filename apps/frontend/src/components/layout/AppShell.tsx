import type { ReactNode } from 'react';

interface AppShellProps {
  children: ReactNode;
}

/** Top-level application layout: sidebar | main | detail-dock | status-bar. */
export function AppShell({ children }: AppShellProps) {
  return (
    <div className="flex flex-col h-screen">
      <div className="flex flex-1 overflow-hidden">{children}</div>
    </div>
  );
}

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
    <footer className="h-8 flex-shrink-0 bg-bg-secondary border-t border-border flex items-center px-3">
      {children}
    </footer>
  );
}

AppShell.Sidebar = Sidebar;
AppShell.Main = Main;
AppShell.RightPanel = RightPanel;
AppShell.StatusBar = StatusBar;
