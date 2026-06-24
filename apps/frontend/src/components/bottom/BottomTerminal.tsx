/**
 * BottomTerminal — the user's own interactive bash shell over SSH.
 *
 * Distinct from the agent session: it spawns a dedicated `bash` session against
 * the active connection so the user can poke at the remote box directly. The
 * session id is tracked in layoutStore.bottomPanelSessionId — deliberately NOT
 * via sessionStore.spawn, which would push the bash session into
 * sessionStore.sessions and hijack activeSessionId (misleading the agent column
 * and status bar into thinking the active session is bash).
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { TerminalInstance } from '../terminal/TerminalInstance';
import { useTerminalApi } from '../../hooks/useTerminalApi';
import { useLayoutStore } from '../../stores/layoutStore';
import { useConnectionStore } from '../../stores/connectionStore';
import { log } from '../../lib/debugLog';

const MAX_AUTO_RETRIES = 10;

export function BottomTerminal() {
  const api = useTerminalApi();
  const sessionId = useLayoutStore((s) => s.bottomPanelSessionId);
  const setSessionId = useLayoutStore((s) => s.setBottomPanelSessionId);

  // Subscribe to stable slices; derive the target connection in render.
  const connections = useConnectionStore((s) => s.connections);
  const activeConnectionId = useConnectionStore((s) => s.activeConnectionId);

  const [spawning, setSpawning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const retriesRef = useRef(0);

  const targetConnId =
    (activeConnectionId && connections[activeConnectionId]?.status === 'connected'
      ? activeConnectionId
      : null) ??
    Object.values(connections).find((c) => c.status === 'connected')?.id ??
    null;

  const openShell = useCallback(async (manual = false) => {
    if (!targetConnId) { log('system', 'BottomTerm: no targetConnId, skipping spawn'); return; }
    if (manual) retriesRef.current = 0;
    setSpawning(true);
    setError(null);
    try {
      log('system', `BottomTerm: spawning bash on conn=${targetConnId} attempt=${retriesRef.current + 1}`);
      const info = await api.spawn(targetConnId, { tool: 'bash' });
      log('system', `BottomTerm: bash spawned session=${info.id.slice(0, 8)}`);
      setSessionId(info.id);
      retriesRef.current = 0;
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      log('system', `BottomTerm: spawn failed (attempt ${retriesRef.current + 1}/${MAX_AUTO_RETRIES}): ${msg}`);
      if (retriesRef.current < MAX_AUTO_RETRIES) {
        retriesRef.current += 1;
        const delay = 300 * retriesRef.current;
        setSpawning(false);
        setTimeout(() => { void openShell(); }, delay);
        return;
      }
      setError(msg);
    } finally {
      setSpawning(false);
    }
  }, [targetConnId, api, setSessionId]);

  // Auto-open a shell once a connection is available and none is running.
  useEffect(() => {
    if (!sessionId && targetConnId && !spawning && !error) {
      void openShell();
    }
  }, [sessionId, targetConnId, spawning, error, openShell]);

  if (!targetConnId) {
    return (
      <div className="flex items-center justify-center h-full">
        <p className="text-xs text-text-secondary italic">连接一台远程主机后即可使用 Bash 终端。</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-2">
        <p className="text-xs text-red-400">无法启动 Bash 终端：{error}</p>
        <button
          onClick={() => openShell(true)}
          className="px-3 py-1 text-xs rounded border border-border text-text-secondary hover:text-text-primary"
        >
          重试
        </button>
      </div>
    );
  }

  if (!sessionId) {
    return (
      <div className="flex items-center justify-center h-full">
        <p className="text-xs text-text-secondary italic">
          {spawning ? '正在启动 Bash 终端…' : '准备终端…'}
        </p>
      </div>
    );
  }

  return <TerminalInstance key={sessionId} sessionId={sessionId} api={api} />;
}
