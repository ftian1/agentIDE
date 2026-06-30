/**
 * ThinkingBlock — collapsible thinking display (secondary).
 *
 * Internal reasoning from the model. Auto-expanded while streaming,
 * collapsed by default for historical blocks to keep the UI focused
 * on primary content (actions / text replies).
 */
import { useState, useEffect } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import type { AgentBlock } from '../../stores/agentStore';

interface Props {
  block: AgentBlock;
  /** When true, the block stays expanded (streaming in progress). */
  isStreaming?: boolean;
}

export function ThinkingBlock({ block, isStreaming }: Props) {
  const [expanded, setExpanded] = useState(false);

  // Auto-expand while streaming; auto-collapse once streaming stops
  // (but leave the latest thinking block expanded).
  useEffect(() => {
    if (isStreaming) setExpanded(true);
  }, [isStreaming]);

  const preview = block.text.length > 120
    ? block.text.slice(0, 120) + '…'
    : block.text;

  return (
    <div className="border-l-2 border-border pl-3 py-1">
      <button
        onClick={() => setExpanded((v) => !v)}
        className="flex items-center gap-1 text-[10px] uppercase tracking-wider text-text-secondary hover:text-text-primary transition-colors w-full text-left"
      >
        {expanded ? (
          <ChevronDown size={10} strokeWidth={1.5} />
        ) : (
          <ChevronRight size={10} strokeWidth={1.5} />
        )}
        思考过程
        {isStreaming && (
          <span className="text-accent animate-pulse ml-1">…</span>
        )}
      </button>
      <p className={`text-xs text-text-secondary whitespace-pre-wrap leading-relaxed mt-0.5 ${expanded ? '' : 'line-clamp-2'}`}>
        {expanded ? block.text : preview}
      </p>
    </div>
  );
}
