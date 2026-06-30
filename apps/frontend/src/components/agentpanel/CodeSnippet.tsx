/**
 * CodeSnippet — monospace code block with configurable line cap.
 *
 * When the code exceeds `maxLines` lines, only the first `maxLines` are
 * shown and a "Show N more lines" toggle reveals the rest.
 */
import { useState, useMemo } from 'react';
import { ChevronDown, ChevronUp } from 'lucide-react';

interface Props {
  code: string;
  /** Maximum visible lines before truncation (default 16). */
  maxLines?: number;
  /** Optional language label shown in the header. */
  language?: string;
  className?: string;
}

export function CodeSnippet({ code, maxLines = 16, language, className }: Props) {
  const [expanded, setExpanded] = useState(false);

  const lines = useMemo(() => code.split('\n'), [code]);
  const truncated = lines.length > maxLines;
  const visible = expanded ? lines : lines.slice(0, maxLines);

  return (
    <div className={`font-mono text-xs rounded border border-border overflow-hidden ${className ?? ''}`}>
      {/* Header bar */}
      {(language || truncated) && (
        <div className="flex items-center justify-between px-2 py-0.5 bg-bg-tertiary border-b border-border">
          <span className="text-[10px] text-text-secondary uppercase tracking-wider">
            {language ?? 'code'}
          </span>
          {truncated && (
            <button
              onClick={() => setExpanded((v) => !v)}
              className="flex items-center gap-1 text-[10px] text-text-secondary hover:text-text-primary transition-colors"
            >
              {expanded ? (
                <>
                  <ChevronUp size={10} strokeWidth={1.5} /> 收起
                </>
              ) : (
                <>
                  <ChevronDown size={10} strokeWidth={1.5} />
                  显示 {lines.length - maxLines} 行
                </>
              )}
            </button>
          )}
        </div>
      )}

      {/* Code body */}
      <pre className="px-2 py-1.5 overflow-x-auto text-text-primary bg-bg-primary whitespace-pre">
        {visible.join('\n')}
        {truncated && !expanded && (
          <span className="text-text-secondary select-none">{'\n...'}</span>
        )}
      </pre>
    </div>
  );
}
