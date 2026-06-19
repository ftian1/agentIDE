/**
 * SessionRail — left sidebar showing active/ended sessions with spawn controls.
 */
import { useState, useCallback } from 'react';
import { useSessionStore } from '../../stores/sessionStore';

interface Props {
  onNewConnection: () => void;
}

export function SessionRail({ onNewConnection }: Props) {
  const sessions = useSessionStore((s) => s.sessions);
  const activeId = useSessionStore((s) => s.activeSessionId);
  const setActive = useSessionStore((s) => s.setActive);
  const spawn = useSessionStore((s) => s.spawn);
  const close = useSessionStore((s) => s.close);
  const [tool, setTool] = useState('echo');
  const [args, setArgs] = useState('Hello from Remote AI IDE');
  const [spawning, setSpawning] = useState(false);

  const sessionList = Object.values(sessions);
  const activeSessions = sessionList.filter((s) => s.state === 'running' || s.state === 'spawning');
  const endedSessions = sessionList.filter((s) => s.state === 'ended');

  const handleSpawn = useCallback(async () => {
    setSpawning(true);
    try {
      await spawn('local', {
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
  }, [tool, args, spawn]);

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-3 border-b border-border">
        <span className="text-xs font-semibold text-text-secondary uppercase tracking-wider">
          Sessions
        </span>
        <button
          onClick={onNewConnection}
          className="text-accent hover:text-blue-300 text-lg leading-none"
          title="New Connection"
        >
          +
        </button>
      </div>

      {/* Quick spawn bar */}
      <div className="p-2 border-b border-border space-y-1.5">
        <input
          type="text"
          value={tool}
          onChange={(e) => setTool(e.target.value)}
          placeholder="command (e.g. bash, cat, echo)"
          className="w-full bg-bg-tertiary text-text-primary text-xs px-2 py-1 rounded border border-border
                     focus:outline-none focus:border-accent placeholder:text-text-secondary"
        />
        <input
          type="text"
          value={args}
          onChange={(e) => setArgs(e.target.value)}
          placeholder="arguments"
          className="w-full bg-bg-tertiary text-text-primary text-xs px-2 py-1 rounded border border-border
                     focus:outline-none focus:border-accent placeholder:text-text-secondary"
        />
        <button
          onClick={handleSpawn}
          disabled={spawning || !tool}
          className="w-full px-2 py-1 text-xs font-medium rounded bg-accent text-white
                     hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
        >
          {spawning ? 'Spawning...' : 'Spawn'}
        </button>
      </div>

      {/* Active sessions */}
      <div className="flex-1 overflow-y-auto px-2 py-2">
        <p className="px-2 py-1 text-xs text-text-secondary uppercase tracking-wider">
          Active ({activeSessions.length})
        </p>
        <div className="space-y-0.5">
          {activeSessions.length === 0 && (
            <p className="px-2 text-xs text-text-secondary italic">No active sessions</p>
          )}
          {activeSessions.map((s) => (
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
              <span className="truncate flex-1">{s.tool}</span>
              <span className="text-text-secondary opacity-50 text-[10px]">#{s.id.slice(0, 6)}</span>
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  close(s.id);
                }}
                className="text-text-secondary hover:text-red-400 text-xs px-0.5"
                title="Close"
              >
                ✕
              </button>
            </button>
          ))}
        </div>

        {/* Ended sessions */}
        {endedSessions.length > 0 && (
          <>
            <p className="px-2 py-1 mt-3 text-xs text-text-secondary uppercase tracking-wider">
              Ended ({endedSessions.length})
            </p>
            <div className="space-y-0.5">
              {endedSessions.map((s) => (
                <div
                  key={s.id}
                  className="px-2 py-1 rounded text-xs text-text-secondary opacity-60 flex items-center gap-2"
                >
                  <span className="w-1.5 h-1.5 rounded-full bg-gray-500 flex-shrink-0" />
                  <span className="truncate flex-1">{s.tool}</span>
                  <span className="text-[10px]">#{s.id.slice(0, 6)}</span>
                </div>
              ))}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
