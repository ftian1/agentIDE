/**
 * SessionManagerPanel — session management sidebar panel.
 *
 * Shows all sessions across all connections with controls to spawn,
 * close, and switch between them. Acts as a centralized session hub.
 */
import { useState, useCallback } from 'react';
import { MonitorPlay, Plus, X, Terminal } from 'lucide-react';
import { useSessionStore } from '../../stores/sessionStore';
import { useConnectionStore } from '../../stores/connectionStore';

export function SessionManagerPanel() {
  const sessions = useSessionStore((s) => s.sessions);
  const activeId = useSessionStore((s) => s.activeSessionId);
  const setActive = useSessionStore((s) => s.setActive);
  const spawn = useSessionStore((s) => s.spawn);
  const close = useSessionStore((s) => s.close);
  const connections = useConnectionStore((s) => s.connections);

  const [tool, setTool] = useState('');
  const [args, setArgs] = useState('');
  const [spawning, setSpawning] = useState(false);

  const sessionList = Object.values(sessions);
  const activeSessions = sessionList.filter(
    (s) => s.state === 'running' || s.state === 'spawning'
  );
  const endedSessions = sessionList.filter((s) => s.state === 'ended');
  const connectedMachines = Object.values(connections).filter(
    (c) => c.status === 'connected'
  );

  const handleSpawn = useCallback(async () => {
    if (!tool) return;
    setSpawning(true);
    try {
      // Use the first active connection, or 'local' fallback
      const connId =
        connectedMachines.length > 0 ? connectedMachines[0].id : 'local';
      await spawn(connId, {
        tool,
        args: args.split(' ').filter(Boolean),
        cwd: undefined,
        env: undefined,
      });
    } catch (e) {
      console.error('Spawn failed:', e);
    } finally {
      setSpawning(false);
    }
  }, [tool, args, spawn, connectedMachines]);

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-3 border-b border-border">
        <MonitorPlay size={14} className="text-accent" />
        <span className="text-xs font-semibold text-text-secondary uppercase tracking-wider">
          Session Manager
        </span>
      </div>

      {/* Quick spawn bar */}
      <div className="p-2 border-b border-border space-y-1.5">
        <div className="flex gap-1.5">
          <input
            type="text"
            value={tool}
            onChange={(e) => setTool(e.target.value)}
            placeholder="Command (e.g. bash, claude)"
            className="flex-1 bg-bg-tertiary text-text-primary text-xs px-2 py-1 rounded border border-border
                       focus:outline-none focus:border-accent placeholder:text-text-secondary"
          />
          <input
            type="text"
            value={args}
            onChange={(e) => setArgs(e.target.value)}
            placeholder="Args"
            className="w-20 bg-bg-tertiary text-text-primary text-xs px-2 py-1 rounded border border-border
                       focus:outline-none focus:border-accent placeholder:text-text-secondary"
          />
        </div>
        <button
          onClick={handleSpawn}
          disabled={spawning || !tool}
          className="w-full flex items-center justify-center gap-1.5 px-2 py-1.5 text-xs font-medium rounded
                     bg-accent text-white hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed
                     transition-colors"
        >
          <Plus size={12} />
          {spawning ? 'Spawning...' : 'Spawn Session'}
        </button>
      </div>

      {/* Session list */}
      <div className="flex-1 overflow-y-auto px-2 py-2 space-y-3">
        {/* Active sessions */}
        <div>
          <p className="px-2 py-1 text-xs text-text-secondary uppercase tracking-wider">
            Active ({activeSessions.length})
          </p>
          <div className="space-y-0.5">
            {activeSessions.length === 0 && (
              <p className="px-2 py-2 text-xs text-text-secondary italic text-center">
                No active sessions
              </p>
            )}
            {activeSessions.map((s) => {
              const conn = connections[s.connectionId];
              return (
                <button
                  key={s.id}
                  onClick={() => setActive(s.id)}
                  className={`
                    w-full text-left px-2 py-1.5 rounded text-xs flex items-center gap-2
                    transition-colors
                    ${s.id === activeId
                      ? 'bg-accent/20 text-text-primary border border-accent/30'
                      : 'hover:bg-bg-tertiary text-text-secondary border border-transparent'
                    }
                  `}
                >
                  <span
                    className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${
                      s.state === 'running' ? 'bg-green-400' : 'bg-yellow-400'
                    }`}
                  />
                  <Terminal size={12} className="flex-shrink-0 opacity-60" />
                  <div className="flex-1 min-w-0">
                    <span className="truncate block text-text-primary">{s.tool}</span>
                    {conn && (
                      <span className="text-[10px] text-text-secondary truncate block">
                        {conn.label || conn.host}
                      </span>
                    )}
                  </div>
                  <span className="text-text-secondary opacity-50 text-[10px]">
                    #{s.id.slice(0, 6)}
                  </span>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      close(s.id);
                    }}
                    className="text-text-secondary hover:text-red-400 p-0.5"
                    title="Close session"
                  >
                    <X size={12} />
                  </button>
                </button>
              );
            })}
          </div>
        </div>

        {/* Ended sessions */}
        {endedSessions.length > 0 && (
          <div>
            <p className="px-2 py-1 text-xs text-text-secondary uppercase tracking-wider">
              Ended ({endedSessions.length})
            </p>
            <div className="space-y-0.5">
              {endedSessions.map((s) => (
                <div
                  key={s.id}
                  className="px-2 py-1 rounded text-xs text-text-secondary opacity-60 flex items-center gap-2"
                >
                  <span className="w-1.5 h-1.5 rounded-full bg-gray-500 flex-shrink-0" />
                  <Terminal size={12} className="flex-shrink-0" />
                  <span className="truncate flex-1">{s.tool}</span>
                  <span className="text-[10px]">#{s.id.slice(0, 6)}</span>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Connection info */}
        {connectedMachines.length === 0 && sessionList.length === 0 && (
          <div className="px-2 py-3 text-center">
            <p className="text-xs text-text-secondary italic">
              Connect to a remote machine via Agent Manager, then spawn sessions here.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
