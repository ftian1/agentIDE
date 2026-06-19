/**
 * ExplorerPanel — combined Sessions + Connections explorer tree.
 */
import { useState } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { SessionRailInner } from '../session/SessionRail';
import { useConnectionStore } from '../../stores/connectionStore';

export function ExplorerPanel({ onNewConnection }: { onNewConnection: () => void }) {
  (window as any).__trackRender('ExplorerPanel');
  const [sessionsOpen, setSessionsOpen] = useState(true);
  const [connectionsOpen, setConnectionsOpen] = useState(true);
  const connections = useConnectionStore((s) => s.connections);
  const connList = Object.values(connections);

  return (
    <div className="flex flex-col h-full overflow-y-auto">
      {/* Sessions section */}
      <button
        onClick={() => setSessionsOpen(!sessionsOpen)}
        className="flex items-center gap-1 px-3 py-2 text-xs font-semibold text-text-secondary
                   uppercase tracking-wider hover:bg-bg-tertiary transition-colors"
      >
        {sessionsOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        Sessions
      </button>
      {sessionsOpen && <SessionRailInner onNewConnection={onNewConnection} />}

      {/* Connections section */}
      <button
        onClick={() => setConnectionsOpen(!connectionsOpen)}
        className="flex items-center gap-1 px-3 py-2 text-xs font-semibold text-text-secondary
                   uppercase tracking-wider hover:bg-bg-tertiary transition-colors border-t border-border"
      >
        {connectionsOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        Remote Machines
      </button>
      {connectionsOpen && (
        <div className="px-2 py-2">
          {connList.length === 0 ? (
            <p className="px-2 text-xs text-text-secondary italic">No connections</p>
          ) : (
            connList.map((c) => (
              <div
                key={c.id}
                className="px-2 py-1 rounded text-xs text-text-secondary flex items-center gap-2"
              >
                <span className={`w-1.5 h-1.5 rounded-full ${
                  c.status === 'connected' ? 'bg-green-400' : 'bg-gray-500'
                }`} />
                <span className="truncate">{c.label}</span>
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}
