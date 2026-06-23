/**
 * AgentStdout — bottom-panel tab showing the live agent/session log feed.
 */
import { useEffect, useRef } from 'react';
import { useAgentLogStore } from '../../stores/agentLogStore';

export function AgentStdout() {
  const lines = useAgentLogStore((s) => s.lines);
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ block: 'end' });
  }, [lines.length]);

  if (lines.length === 0) {
    return (
      <div className="flex items-center justify-center h-full">
        <p className="text-xs text-text-secondary italic">等待 Agent 输出...</p>
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto px-3 py-2 font-mono text-xs leading-relaxed">
      {lines.map((l) => (
        <div
          key={l.id}
          className={l.channel === 'agent' ? 'text-text-primary' : 'text-text-secondary'}
        >
          {l.text}
        </div>
      ))}
      <div ref={endRef} />
    </div>
  );
}
