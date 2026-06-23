/**
 * TerminalPane — tabbed container for terminal instances.
 *
 * Shows a tab bar when multiple sessions are active.
 * Displays an empty state when no sessions exist.
 */

import { TerminalInstance } from './TerminalInstance';
import { useSessionStore } from '../../stores/sessionStore';
import { useTerminalApi } from '../../hooks/useTerminalApi';

export function TerminalPane() {
  const sessions = useSessionStore((s) => s.sessions);
  const activeId = useSessionStore((s) => s.activeSessionId);
  const setActive = useSessionStore((s) => s.setActive);
  const api = useTerminalApi();

  const sessionList = Object.values(sessions);
  const activeSession = activeId ? sessions[activeId] : null;

  if (sessionList.length === 0) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center bg-bg-primary">
        <div className="text-center space-y-4">
          <div className="text-5xl text-text-secondary opacity-20">▸</div>
          <div>
            <p className="text-text-secondary text-sm font-medium">No active sessions</p>
            <p className="text-text-secondary text-xs mt-1">
              Spawn a session via the sidebar or press{' '}
              <kbd className="px-1 py-0.5 text-xs bg-bg-tertiary rounded border border-border">
                Ctrl+Shift+N
              </kbd>
            </p>
          </div>
        </div>
      </div>
    );
  }

  const activeSessions = sessionList.filter(
    (s) => s.state === 'running' || s.state === 'spawning'
  );

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      {/* Tab bar — always visible when sessions exist */}
      <div className="flex items-center bg-bg-secondary border-b border-border px-1">
        {activeSessions.map((s) => (
          <div
            key={s.id}
            className={`flex items-center gap-1 px-3 py-1.5 text-xs border-b-2 transition-colors group
              ${s.id === activeId
                ? 'border-accent text-text-primary'
                : 'border-transparent text-text-secondary hover:text-text-primary cursor-pointer'
              }`}
          >
            <button
              onClick={() => setActive(s.id)}
              className="flex items-center gap-1.5"
            >
              <span className="w-1.5 h-1.5 rounded-full flex-shrink-0 bg-green-400" />
              {s.tool}
              <span className="text-text-secondary opacity-50">#{s.id.slice(0, 6)}</span>
            </button>
            <button
              onClick={(e) => {
                e.stopPropagation();
                useSessionStore.getState().close(s.id);
              }}
              className="ml-1 p-0.5 rounded hover:bg-red-900/30 text-text-secondary hover:text-red-400 opacity-0 group-hover:opacity-100 transition-opacity"
              title="Close session"
            >
              <svg width="10" height="10" viewBox="0 0 10 10"><path d="M1 1l8 8M9 1L1 9" stroke="currentColor" strokeWidth="1.5"/></svg>
            </button>
          </div>
        ))}
      </div>

      {/* Terminal area — only render the active terminal. */}
      <div className="flex-1">
        {activeSession ? (
          <TerminalInstance
            key={activeSession.id}
            sessionId={activeSession.id}
            api={api}
            onReady={(cols, rows) => {
              console.log(`Terminal ready: ${activeSession.id} ${cols}x${rows}`);
            }}
          />
        ) : (
          <div className="flex items-center justify-center h-full">
            <p className="text-xs text-text-secondary italic">
              {activeSessions.length > 0 ? 'Select a session tab above.' : 'No active sessions.'}
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
