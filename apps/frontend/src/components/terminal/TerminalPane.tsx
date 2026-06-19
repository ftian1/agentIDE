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

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      {/* Tab bar */}
      {sessionList.length > 1 && (
        <div className="flex items-center bg-bg-secondary border-b border-border px-1">
          {sessionList.map((s) => (
            <button
              key={s.id}
              onClick={() => setActive(s.id)}
              className={`
                px-3 py-1.5 text-xs border-b-2 transition-colors
                ${s.id === activeId
                  ? 'border-accent text-text-primary'
                  : 'border-transparent text-text-secondary hover:text-text-primary'
                }
              `}
            >
              {s.tool}
              <span className="ml-2 text-text-secondary opacity-50">#{s.id.slice(0, 6)}</span>
            </button>
          ))}
        </div>
      )}

      {/* Terminal area */}
      <div className="flex-1 relative">
        {sessionList.map((s) => (
          <div
            key={s.id}
            className="absolute inset-0"
            style={{ display: s.id === activeId ? 'block' : 'none' }}
          >
            <TerminalInstance
              sessionId={s.id}
              api={api}
              onReady={(cols, rows) => {
                console.log(`Terminal ready: ${s.id} ${cols}x${rows}`);
              }}
            />
          </div>
        ))}
      </div>
    </div>
  );
}
