/**
 * AgentTurn — renders a single conversation turn (user message or agent blocks).
 */
import type { AgentBlock, AgentTurn as Turn } from '../../stores/agentStore';

const BORDER_BY_KIND: Record<AgentBlock['kind'], string> = {
  thought: 'border-border',
  action: 'border-accent',
  observation: 'border-green-500/50',
};

function Block({ block }: { block: AgentBlock }) {
  return (
    <div className={`border-l-2 pl-2.5 ${BORDER_BY_KIND[block.kind]}`}>
      <div className="flex items-center gap-1.5">
        {block.kind === 'observation' && (
          <span className="w-1.5 h-1.5 rounded-full bg-green-400" />
        )}
        <span className="text-[10px] uppercase tracking-wider text-text-secondary">
          {block.kind}
        </span>
        {block.status && (
          <span className="text-[10px] text-text-secondary opacity-70">· {block.status}</span>
        )}
      </div>

      {block.label && (
        <p className="text-xs text-text-primary mt-1">{block.label}</p>
      )}
      {block.text && (
        <p className="text-xs text-text-secondary mt-1 whitespace-pre-wrap">{block.text}</p>
      )}
      {block.code && (
        <pre className="font-mono text-xs bg-bg-primary rounded p-2 mt-1 overflow-x-auto text-text-primary">
          {block.code}
        </pre>
      )}
    </div>
  );
}

export function AgentTurn({ turn }: { turn: Turn }) {
  if (turn.role === 'user') {
    return (
      <div className="bg-bg-tertiary rounded px-3 py-2 text-sm text-text-primary whitespace-pre-wrap">
        {turn.text}
      </div>
    );
  }

  return (
    <div className="space-y-2.5">
      {turn.blocks.map((b) => (
        <Block key={b.id} block={b} />
      ))}
    </div>
  );
}
