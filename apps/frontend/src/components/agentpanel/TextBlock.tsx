/**
 * TextBlock — AI text reply (primary content, normal visual weight).
 *
 * Renders the agent's written response to the user. This is the main
 * content the user cares about — distinct from internal thinking.
 */
import type { AgentBlock } from '../../stores/agentStore';

interface Props {
  block: AgentBlock;
}

export function TextBlock({ block }: Props) {
  return (
    <div className="border-l-2 border-accent/50 pl-3 py-1">
      <div className="flex items-center gap-1.5 mb-0.5">
        <span className="text-[10px] uppercase tracking-wider text-text-secondary">
          回复
        </span>
      </div>
      <p className="text-sm text-text-primary whitespace-pre-wrap leading-relaxed">
        {block.text}
      </p>
    </div>
  );
}
