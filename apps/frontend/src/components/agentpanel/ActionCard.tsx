/**
 * ActionCard — tool invocation card (PRIMARY).
 *
 * The most visually prominent block type. Shows:
 * - Status dot (pulsing while running, static when done/error)
 * - Tool name and human-readable label
 * - Code/command in a CodeSnippet (truncatable)
 * - Clickable file paths (when the label matches Read/Edit/Write patterns)
 */
import { useMemo } from 'react';
import { Wrench, Terminal, FileText, Search, Globe } from 'lucide-react';
import { StatusDot, statusForAction, type DotStatus } from './StatusDot';
import { CodeSnippet } from './CodeSnippet';
import { parseActionLabel } from '../../stores/contextFileStore';
import type { AgentBlock } from '../../stores/agentStore';

/** Icon per tool category (heuristic from label prefix). */
const TOOL_ICON_MAP: Record<string, typeof Wrench> = {
  bash: Terminal,
  read: FileText,
  edit: FileText,
  write: FileText,
  grep: Search,
  glob: Search,
  websearch: Globe,
  webfetch: Globe,
};

interface Props {
  block: AgentBlock;
  /** Derived status for this action. */
  status: DotStatus;
  /** Click handler for file paths in the label. */
  onOpenFile?: (path: string) => void;
}

export function ActionCard({ block, status, onOpenFile }: Props) {
  // Parse file path from label (e.g. "Read src/main.rs" → { origin: 'read', path: 'src/main.rs' })
  const fileRef = useMemo(() => parseActionLabel(block.label), [block.label]);

  // Pick an icon based on the tool name prefix.
  const toolVerb = (block.label ?? '').split(/[:\s]/)[0]?.toLowerCase() ?? '';
  const ToolIcon = TOOL_ICON_MAP[toolVerb] ?? Wrench;

  // Determine a language hint for the code snippet.
  const langHint = toolVerb === 'bash' ? 'bash' : undefined;

  return (
    <div className="border-l-2 border-accent pl-3 py-1.5">
      {/* Header: status dot + icon + tool label */}
      <div className="flex items-center gap-2">
        <StatusDot status={status} />
        <ToolIcon size={12} strokeWidth={1.5} className="text-text-secondary flex-shrink-0" />
        <span className="text-xs text-text-primary font-medium truncate">
          {block.label || 'Tool'}
        </span>
        <StatusBadge status={status} />
      </div>

      {/* File path as clickable link */}
      {fileRef && onOpenFile && (
        <button
          onClick={() => onOpenFile(fileRef.path)}
          className="text-[11px] text-accent hover:underline mt-1 block text-left truncate max-w-full"
          title={`Open ${fileRef.path}`}
        >
          {fileRef.path}
        </button>
      )}
      {fileRef && !onOpenFile && (
        <p className="text-[11px] text-accent/70 mt-1 truncate">{fileRef.path}</p>
      )}

      {/* Code / command body */}
      {block.code && (
        <div className="mt-1.5">
          <CodeSnippet code={block.code} language={langHint} maxLines={16} />
        </div>
      )}
    </div>
  );
}

/** Small inline badge showing the status. */
function StatusBadge({ status }: { status: DotStatus }) {
  if (status === 'running') {
    return (
      <span className="text-[9px] px-1 py-px rounded bg-accent/20 text-accent animate-pulse">
        执行中
      </span>
    );
  }
  if (status === 'done') {
    return (
      <span className="text-[9px] px-1 py-px rounded bg-green-500/20 text-green-400">
        完成
      </span>
    );
  }
  if (status === 'error') {
    return (
      <span className="text-[9px] px-1 py-px rounded bg-red-500/20 text-red-400">
        失败
      </span>
    );
  }
  return null;
}
