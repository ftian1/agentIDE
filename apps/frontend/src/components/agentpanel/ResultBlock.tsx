/**
 * ResultBlock — tool execution result display (PRIMARY).
 *
 * Shows the output of a tool call. Success (Observation) renders with a
 * green left border. Error blocks render with a red left border.
 */
import { useState } from 'react';
import { ChevronDown, ChevronUp, CheckCircle, AlertCircle } from 'lucide-react';
import type { AgentBlock } from '../../stores/agentStore';

interface Props {
  block: AgentBlock;
}

const MAX_LINES = 24;

export function ResultBlock({ block }: Props) {
  const isError = block.kind === 'error';
  const [expanded, setExpanded] = useState(false);

  const lines = block.text.split('\n');
  const long = lines.length > MAX_LINES;
  const visible = expanded || !long ? block.text : lines.slice(0, MAX_LINES).join('\n');

  return (
    <div className={`border-l-2 pl-3 py-1.5 ${isError ? 'border-red-500' : 'border-green-500/50'}`}>
      {/* Header */}
      <div className="flex items-center gap-1.5">
        {isError ? (
          <AlertCircle size={12} strokeWidth={1.5} className="text-red-400 flex-shrink-0" />
        ) : (
          <CheckCircle size={12} strokeWidth={1.5} className="text-green-400 flex-shrink-0" />
        )}
        <span className={`text-[10px] uppercase tracking-wider ${isError ? 'text-red-400' : 'text-text-secondary'}`}>
          {isError ? '错误' : '结果'}
        </span>
      </div>

      {/* Body */}
      <pre className={`text-xs mt-1 whitespace-pre-wrap leading-relaxed overflow-x-auto ${isError ? 'text-red-300' : 'text-text-primary'}`}>
        {visible}
        {long && !expanded && (
          <span className="text-text-secondary">{'\n...'}</span>
        )}
      </pre>

      {/* Show more / less toggle for long output */}
      {long && (
        <button
          onClick={() => setExpanded((v) => !v)}
          className="flex items-center gap-1 text-[10px] text-text-secondary hover:text-text-primary mt-1 transition-colors"
        >
          {expanded ? (
            <>
              <ChevronUp size={10} strokeWidth={1.5} /> 收起
            </>
          ) : (
            <>
              <ChevronDown size={10} strokeWidth={1.5} />
              显示全部 ({lines.length} 行)
            </>
          )}
        </button>
      )}
    </div>
  );
}
