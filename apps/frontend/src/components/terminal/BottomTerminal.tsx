/**
 * BottomTerminal — dedicated bash shell on the remote host.
 * Separate from the main editor's session tabs.
 * Auto-spawns a bash session when a connection is active.
 */
import { useEffect } from 'react';
import { TerminalInstance } from './TerminalInstance';
import { useTerminalApi } from '../../hooks/useTerminalApi';
import { useLayoutStore } from '../../stores/layoutStore';
import { useConnectionStore } from '../../stores/connectionStore';
import { useSessionStore } from '../../stores/sessionStore';

export function BottomTerminal() {
  const api = useTerminalApi();
  const bottomSessionId = useLayoutStore((s) => s.bottomPanelSessionId);
  const setBottomSessionId = useLayoutStore((s) => s.setBottomPanelSessionId);
  const connections = useConnectionStore((s) => s.connections);
  const sessions = useSessionStore((s) => s.sessions);
  const spawn = useSessionStore((s) => s.spawn);

  // Find first connected machine
  const connected = Object.values(connections).find((c) => c.status === 'connected');

  // Auto-spawn bash if connected and no bottom session yet
  useEffect(() => {
    if (!connected) return;
    if (bottomSessionId && sessions[bottomSessionId]) return;

    let cancelled = false;
    (async () => {
      try {
        const info = await spawn(connected.id, {
          tool: 'bash',
          args: [],
          cwd: undefined,
          env: undefined,
        });
        if (!cancelled) setBottomSessionId(info.id);
      } catch (e) {
        console.error('Bottom terminal spawn failed:', e);
      }
    })();
    return () => { cancelled = true; };
  }, [connected?.id, bottomSessionId, !!sessions[bottomSessionId!]]); // eslint-disable-line

  if (!connected) {
    return (
      <div className="flex items-center justify-center h-full">
        <p className="text-xs text-text-secondary italic">
          Connect to a remote machine to open a terminal.
        </p>
      </div>
    );
  }

  if (!bottomSessionId || !sessions[bottomSessionId]) {
    return (
      <div className="flex items-center justify-center h-full">
        <p className="text-xs text-text-secondary italic">Opening bash shell...</p>
      </div>
    );
  }

  return (
    <TerminalInstance
      key={bottomSessionId}
      sessionId={bottomSessionId}
      api={api}
      onReady={(cols, rows) => {
        console.log(`Bottom terminal ready: ${cols}x${rows}`);
      }}
    />
  );
}
