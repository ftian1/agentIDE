/**
 * Dashboard — overview when no session is active.
 */
import { useConnectionStore } from '../../stores/connectionStore';
import { useSessionStore } from '../../stores/sessionStore';

export function Dashboard() {
  const connections = useConnectionStore((s) => Object.values(s.connections));
  const sessions = useSessionStore((s) => Object.values(s.sessions));

  const activeSessions = sessions.filter((s) => s.state === 'running');
  const connectedCount = connections.filter((c) => c.status === 'connected').length;

  return (
    <div className="flex-1 flex flex-col items-center justify-center bg-bg-primary">
      <div className="text-center space-y-6 max-w-md">
        <div className="text-5xl text-text-secondary opacity-20">▸</div>
        <div>
          <h2 className="text-text-primary text-lg font-semibold">Remote AI IDE</h2>
          <p className="text-text-secondary text-sm mt-1">
            AI-powered remote development environment
          </p>
        </div>

        <div className="grid grid-cols-3 gap-4">
          <div className="bg-bg-secondary border border-border rounded-lg p-3">
            <div className="text-2xl font-mono text-accent">{connections.length}</div>
            <div className="text-xs text-text-secondary mt-0.5">Connections</div>
          </div>
          <div className="bg-bg-secondary border border-border rounded-lg p-3">
            <div className="text-2xl font-mono text-green-400">{connectedCount}</div>
            <div className="text-xs text-text-secondary mt-0.5">Online</div>
          </div>
          <div className="bg-bg-secondary border border-border rounded-lg p-3">
            <div className="text-2xl font-mono text-yellow-400">{activeSessions.length}</div>
            <div className="text-xs text-text-secondary mt-0.5">Sessions</div>
          </div>
        </div>

        <div className="text-xs text-text-secondary space-y-1">
          <p>
            <kbd className="px-1 py-0.5 bg-bg-tertiary rounded border border-border">Ctrl+Shift+N</kbd>
            {' '}New session
          </p>
          <p>Spawn a local command from the sidebar, or connect to a remote host.</p>
        </div>
      </div>
    </div>
  );
}
