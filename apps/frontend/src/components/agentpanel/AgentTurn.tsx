/**
 * AgentTurn — renders a single conversation turn.
 *
 * Dispatches each {@link AgentBlock} to the appropriate visual component:
 * - User turns  → simple rounded bubble
 * - Agent turns → kind-dispatching block renderer
 *
 * The primary / secondary visual hierarchy is enforced here:
 * - TextBlock, ActionCard, ResultBlock — PRIMARY (full visual weight)
 * - ThinkingBlock — SECONDARY (collapsible, gray)
 * - LifecycleBanner — de-emphasised divider
 */
import type { AgentBlock, AgentTurn as Turn } from '../../stores/agentStore';
import { TextBlock } from './TextBlock';
import { ThinkingBlock } from './ThinkingBlock';
import { ActionCard } from './ActionCard';
import { ResultBlock } from './ResultBlock';
import { LifecycleBanner } from './LifecycleBanner';
import { statusForAction } from './StatusDot';

interface Props {
  turn: Turn;
  /** Whether this is the last turn (still streaming). */
  isStreaming?: boolean;
  /** Click handler for file paths in action labels. */
  onOpenFile?: (path: string) => void;
}

export function AgentTurn({ turn, isStreaming, onOpenFile }: Props) {
  // ── User turn ──
  if (turn.role === 'user') {
    return (
      <div className="bg-bg-tertiary rounded-lg px-3 py-2 text-sm text-text-primary whitespace-pre-wrap">
        {turn.text}
      </div>
    );
  }

  // ── Agent turn ──
  const blocks = turn.blocks;
  if (blocks.length === 0) return null;

  // Determine which blocks count as "streaming" (the last thinking block
  // if the turn is still open).
  const lastThinkIdx = findLastIndex(blocks, (b) => b.kind === 'thought');

  return (
    <div className="space-y-2">
      {blocks.map((block, idx) => {
        // Check if this thinking block is actively streaming.
        const thinkStreaming = isStreaming && idx === lastThinkIdx;

        switch (block.kind) {
          case 'text':
            return <TextBlock key={block.id} block={block} />;

          case 'thought':
            return (
              <ThinkingBlock
                key={block.id}
                block={block}
                isStreaming={thinkStreaming}
              />
            );

          case 'action': {
            const status = statusForAction(block, blocks, idx);
            return (
              <ActionCard
                key={block.id}
                block={block}
                status={status}
                onOpenFile={onOpenFile}
              />
            );
          }

          case 'observation':
            return <ResultBlock key={block.id} block={block} />;

          case 'error':
            return <ResultBlock key={block.id} block={block} />;

          case 'unknown':
            // Raw JSON dump — keep it compact.
            return (
              <div key={block.id} className="border-l-2 border-yellow-500/50 pl-2.5 py-1">
                <span className="text-[10px] uppercase tracking-wider text-text-secondary">
                  未知事件
                </span>
                {block.label && (
                  <p className="text-[10px] text-text-secondary mt-0.5">{block.label}</p>
                )}
                <pre className="text-[10px] text-text-secondary/70 mt-0.5 whitespace-pre-wrap break-all max-h-20 overflow-y-auto">
                  {block.text}
                </pre>
              </div>
            );

          default:
            return null;
        }
      })}
    </div>
  );
}

/** Find the last index in an array satisfying a predicate, or -1. */
function findLastIndex<T>(arr: T[], pred: (item: T) => boolean): number {
  for (let i = arr.length - 1; i >= 0; i--) {
    if (pred(arr[i])) return i;
  }
  return -1;
}
