/**
 * SessionDetail — right-side dock with tabbed detail views.
 */
import { useState } from 'react';
import { useSessionStore } from '../../stores/sessionStore';

type Tab = 'tasks' | 'subagents' | 'turns' | 'cost';

export function SessionDetail() {
  const [tab, setTab] = useState<Tab>('turns');
  const activeSession = useSessionStore((s) => {
    const id = s.activeSessionId;
    return id ? s.sessions[id] : null;
  });

  if (!activeSession) {
    return (
      <div className="p-3">
        <p className="text-xs text-text-secondary italic">Select a session to view details.</p>
      </div>
    );
  }

  const tabs: { key: Tab; label: string }[] = [
    { key: 'tasks', label: 'Tasks' },
    { key: 'subagents', label: 'Subagents' },
    { key: 'turns', label: 'Turns' },
    { key: 'cost', label: 'Cost' },
  ];

  return (
    <div className="flex flex-col h-full">
      {/* Tab bar */}
      <div className="flex border-b border-border">
        {tabs.map((t) => (
          <button
            key={t.key}
            onClick={() => setTab(t.key)}
            className={`flex-1 px-2 py-1.5 text-xs border-b-2 transition-colors ${
              tab === t.key
                ? 'border-accent text-text-primary'
                : 'border-transparent text-text-secondary hover:text-text-primary'
            }`}
          >
            {t.label}
          </button>
        ))}
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-3">
        {tab === 'turns' && (
          <div className="space-y-2">
            <div className="flex justify-between text-xs">
              <span className="text-text-secondary">Total turns</span>
              <span className="text-text-primary font-mono">{activeSession.turnCount}</span>
            </div>
            <div className="flex justify-between text-xs">
              <span className="text-text-secondary">Session ID</span>
              <span className="text-text-primary font-mono text-[10px]">{activeSession.id.slice(0, 12)}...</span>
            </div>
            <div className="flex justify-between text-xs">
              <span className="text-text-secondary">State</span>
              <span className="text-text-primary">{activeSession.state}</span>
            </div>
            <div className="flex justify-between text-xs">
              <span className="text-text-secondary">PID</span>
              <span className="text-text-primary font-mono">{activeSession.pid}</span>
            </div>
          </div>
        )}

        {tab === 'cost' && (
          <div className="space-y-2">
            <div className="flex justify-between text-xs">
              <span className="text-text-secondary">Input tokens</span>
              <span className="text-text-primary font-mono">{activeSession.cost.inputTokens.toLocaleString()}</span>
            </div>
            <div className="flex justify-between text-xs">
              <span className="text-text-secondary">Output tokens</span>
              <span className="text-text-primary font-mono">{activeSession.cost.outputTokens.toLocaleString()}</span>
            </div>
            <div className="flex justify-between text-xs">
              <span className="text-text-secondary">Cost</span>
              <span className="text-text-primary font-mono">${activeSession.cost.costUsd.toFixed(4)}</span>
            </div>
          </div>
        )}

        {tab === 'tasks' && (
          <p className="text-xs text-text-secondary italic">Tool calls will appear here.</p>
        )}

        {tab === 'subagents' && (
          <p className="text-xs text-text-secondary italic">Sub-agents will appear here.</p>
        )}
      </div>
    </div>
  );
}
