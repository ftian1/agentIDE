import { useConnectionStore } from '../../stores/connectionStore';
import { useSessionStore } from '../../stores/sessionStore';
import { useAgentStore, deriveAgentActivity } from '../../stores/agentStore';
import { usePerfStore, formatMem } from '../../stores/perfStore';

interface Props {
  onToggleDetail: () => void;
}

/**
 * Bottom status bar: connection node · backend agent state · remote perf monitor.
 *
 * Example: ● user@host · Claude Code: [Executing Tool: Bash] · CPU 45% | MEM 3.2G | Disk IO: Normal
 */
export function StatusPanel({ onToggleDetail }: Props) {
  const connections = useConnectionStore((s) => s.connections);
  const activeConnectionId = useConnectionStore((s) => s.activeConnectionId);
  const sessions = useSessionStore((s) => s.sessions);
  const activeSessionId = useSessionStore((s) => s.activeSessionId);

  const agentActivity = useAgentStore((s) => deriveAgentActivity(s, activeSessionId));
  const perf = usePerfStore((s) => (activeConnectionId ? s.byConnection[activeConnectionId] : undefined));

  const connList = Object.values(connections);
  const connectedCount = connList.filter((c) => c.status === 'connected').length;
  const hasConnection = connectedCount > 0;

  // Resolve the node label for the active (or first connected) connection.
  const activeConn =
    (activeConnectionId && connections[activeConnectionId]) ||
    connList.find((c) => c.status === 'connected') ||
    null;
  const nodeLabel = activeConn ? `${activeConn.user}@${activeConn.host}` : 'No connection';

  // Resolve the active session's tool for the agent-state label.
  const activeSession = activeSessionId ? sessions[activeSessionId] : null;
  const toolName =
    activeSession?.tool === 'claude'
      ? 'Claude Code'
      : activeSession?.tool === 'copilot'
      ? 'Copilot'
      : activeSession?.tool
      ? activeSession.tool.charAt(0).toUpperCase() + activeSession.tool.slice(1)
      : null;

  return (
    <div className="flex items-center gap-3 w-full text-xs text-text-secondary">
      {/* Connection node */}
      <div className="flex items-center gap-1.5 flex-shrink-0">
        <span className={`w-1.5 h-1.5 rounded-full ${hasConnection ? 'bg-green-400' : 'bg-gray-500'}`} />
        <span className="font-mono">{nodeLabel}</span>
      </div>

      {/* Agent state */}
      {toolName && (
        <>
          <span className="opacity-40">·</span>
          <div className="flex items-center gap-1 flex-shrink-0">
            <span className="text-text-primary">{toolName}:</span>
            <span className="text-accent">[{agentActivity}]</span>
          </div>
        </>
      )}

      {/* Spacer */}
      <div className="flex-1" />

      {/* Remote perf monitor */}
      {perf && (
        <div className="flex items-center gap-2 flex-shrink-0 font-mono">
          <span className={perf.cpuPercent >= 85 ? 'text-red-400' : perf.cpuPercent >= 60 ? 'text-yellow-400' : ''}>
            CPU {Math.round(perf.cpuPercent)}%
          </span>
          <span className="opacity-40">|</span>
          <span>
            MEM {formatMem(perf.memUsedMb)}
            <span className="opacity-50">/{formatMem(perf.memTotalMb)}</span>
          </span>
          <span className="opacity-40">|</span>
          <span className={perf.diskIo === 'Busy' ? 'text-yellow-400' : ''}>
            Disk IO: {perf.diskIo}
          </span>
        </div>
      )}

      {/* Detail toggle */}
      <button onClick={onToggleDetail} className="hover:text-text-primary transition-colors flex-shrink-0">
        Toggle Detail
      </button>
    </div>
  );
}
