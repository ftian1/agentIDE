/**
 * ActiveFilesMenu — "Show Active Files" dropdown for the terminal column header.
 *
 * Lists the files currently in the agent CLI's working context (derived from
 * its Read/Edit/Write tool-use stream). Each entry has an ✗ to kick it out of
 * the displayed context. Kicking is UI-only for now (see contextFileStore).
 */
import { useEffect, useMemo, useRef, useState } from 'react';
import { FileText, FilePen, ChevronDown, X } from 'lucide-react';
import { useContextFileStore, selectActiveFiles, type ContextFile } from '../../stores/contextFileStore';

interface Props {
  sessionId: string | null;
}

export function ActiveFilesMenu({ sessionId }: Props) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  // Subscribe to STABLE store slices only. Returning a derived array straight
  // from the selector breaks React 18's useSyncExternalStore snapshot identity
  // check (new array every render → infinite re-render, React #185). Derive the
  // filtered/sorted list in useMemo instead, off the stable references.
  const bySession = useContextFileStore((s) => s.bySession);
  const kicked = useContextFileStore((s) => s.kicked);
  const kickFile = useContextFileStore((s) => s.kickFile);

  const files = useMemo(
    () => selectActiveFiles({ bySession, kicked }, sessionId),
    [bySession, kicked, sessionId],
  );

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener('mousedown', onDown);
    return () => window.removeEventListener('mousedown', onDown);
  }, [open]);

  const disabled = !sessionId;

  return (
    <div className="relative" ref={ref}>
      <button
        onClick={() => setOpen((v) => !v)}
        disabled={disabled}
        className="flex items-center gap-1 px-2 py-0.5 text-xs rounded text-text-secondary
                   hover:text-text-primary hover:bg-bg-tertiary transition-colors
                   disabled:opacity-40 disabled:cursor-not-allowed"
        title="Files in the agent's context"
      >
        <FileText size={12} />
        <span>Active Files</span>
        {files.length > 0 && (
          <span className="text-[10px] px-1 rounded bg-accent/20 text-accent">{files.length}</span>
        )}
        <ChevronDown size={11} className={`transition-transform ${open ? 'rotate-180' : ''}`} />
      </button>

      {open && (
        <div className="absolute right-0 top-full mt-1 w-72 max-h-80 overflow-y-auto z-50
                        bg-bg-secondary border border-border rounded shadow-lg py-1">
          <div className="px-3 py-1.5 text-[10px] uppercase tracking-wider text-text-secondary border-b border-border/60">
            In Context · {files.length} file{files.length !== 1 ? 's' : ''}
          </div>
          {files.length === 0 ? (
            <div className="px-3 py-3 text-xs text-text-secondary italic">
              No files in context yet. They appear as the agent reads or edits files.
            </div>
          ) : (
            files.map((f) => (
              <ContextFileRow
                key={f.path}
                file={f}
                onKick={() => sessionId && kickFile(sessionId, f.path)}
              />
            ))
          )}
        </div>
      )}
    </div>
  );
}

function ContextFileRow({ file, onKick }: { file: ContextFile; onKick: () => void }) {
  const name = file.path.split('/').pop() || file.path;
  const dir = file.path.slice(0, file.path.length - name.length);
  const Icon = file.origin === 'read' ? FileText : FilePen;
  const iconColor = file.origin === 'read' ? 'text-text-secondary' : 'text-yellow-400';

  return (
    <div className="group flex items-center gap-2 px-3 py-1.5 hover:bg-bg-tertiary">
      <Icon size={13} className={`flex-shrink-0 ${iconColor}`} />
      <div className="flex-1 min-w-0">
        <div className="text-xs text-text-primary truncate">{name}</div>
        {dir && <div className="text-[10px] text-text-secondary truncate opacity-60">{dir}</div>}
      </div>
      <button
        onClick={onKick}
        title="Remove from context"
        className="p-0.5 rounded text-text-secondary hover:text-red-400 hover:bg-red-900/30
                   opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0"
      >
        <X size={13} />
      </button>
    </div>
  );
}
