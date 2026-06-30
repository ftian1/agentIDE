/**
 * StatusDot — animated status indicator for agent blocks.
 *
 * Used across the visual interaction layer to show whether a tool call is
 * running, completed, errored, or idle.
 */
import type { AgentBlock } from '../../stores/agentStore';

export type DotStatus = 'running' | 'done' | 'error' | 'idle';

interface Props {
  status: DotStatus;
  className?: string;
}

/** Tailwind classes per status. */
const DOT_CLASS: Record<DotStatus, string> = {
  running: 'bg-green-400 animate-pulse',
  done: 'bg-green-400',
  error: 'bg-red-500',
  idle: 'bg-gray-500',
};

export function StatusDot({ status, className }: Props) {
  return (
    <span
      className={`inline-block w-1.5 h-1.5 rounded-full flex-shrink-0 ${DOT_CLASS[status]} ${className ?? ''}`}
    />
  );
}

/**
 * Derive a {@link DotStatus} for an action-style block based on whether an
 * observation or error follows later in the turn.
 *
 * An action that is the LAST block in a turn is considered still "running".
 * An action followed by an observation is "done".
 * An action followed by an error block is "error".
 * Otherwise "idle".
 */
export function statusForAction(
  block: AgentBlock,
  allBlocks: AgentBlock[],
  blockIndex: number,
): DotStatus {
  // If this block has an explicit status string, respect it.
  if (block.status === 'running') return 'running';
  if (block.status === 'done') return 'done';
  if (block.status === 'error') return 'error';

  // Heuristic: look ahead for the outcome.
  for (let i = blockIndex + 1; i < allBlocks.length; i++) {
    const next = allBlocks[i];
    if (next.kind === 'action') {
      // Another action started — this one is done (no explicit error/obs).
      return 'done';
    }
    if (next.kind === 'observation') return 'done';
    if (next.kind === 'error') return 'error';
  }
  // No follow-up: still running.
  return 'running';
}
