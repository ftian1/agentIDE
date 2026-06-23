/**
 * CodeChangeEditor — inline patch-preview (staged changes) workspace.
 *
 * Instead of a read-only side-by-side diff, this renders a unified inline diff
 * with red/green line highlights and per-hunk ✓/✗ controls in the line gutter.
 * Accepting a hunk stages it; "Apply" writes the accepted hunks back to the
 * remote file. This is the "patch real-time preview" view — the agent's edits
 * are shown like a git diff and applied granularly, never blindly overwritten.
 *
 * Keyboard shortcuts:
 *   Ctrl+Enter — accept current file (all hunks) and apply
 *   Ctrl+Backspace — reject current file (all hunks)
 */
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useCodeChangeStore, fileHunks, type FileChange } from '../../stores/codeChangeStore';
import { useLayoutStore } from '../../stores/layoutStore';
import { computeDiff, type UnifiedLine } from '../../lib/diff';

export function CodeChangeEditor() {
  const changeSets = useCodeChangeStore((s) => s.changeSets);
  const activeChangeSetId = useCodeChangeStore((s) => s.activeChangeSetId);
  const setHunkDecision = useCodeChangeStore((s) => s.setHunkDecision);
  const applyAcceptedHunks = useCodeChangeStore((s) => s.applyAcceptedHunks);

  const editorTabs = useLayoutStore((s) => s.editorTabs);
  const activeTabId = useLayoutStore((s) => s.activeEditorTabId);
  const addTab = useLayoutStore((s) => s.addEditorTab);

  const [applyState, setApplyState] = useState<'idle' | 'applying' | 'error'>('idle');
  const [applyError, setApplyError] = useState<string | null>(null);

  const activeTab = editorTabs.find((t) => t.id === activeTabId);
  const activeChangeSet = activeTab?.changeSetId ? changeSets[activeTab.changeSetId] : null;
  const activeFile = activeChangeSet && activeTab ? activeChangeSet.files[activeTab.filePath] : null;

  // Open files from active change set into editor tabs.
  useEffect(() => {
    if (!activeChangeSetId || !changeSets[activeChangeSetId]) return;
    const cs = changeSets[activeChangeSetId];
    for (const [path, file] of Object.entries(cs.files)) {
      addTab({
        id: file.id,
        filePath: path,
        label: path.split('/').pop() || path,
        icon: file.oldContent === null ? 'add' : file.newContent === null ? 'delete' : 'modify',
        changeSetId: cs.id,
      });
    }
  }, [activeChangeSetId]); // eslint-disable-line react-hooks/exhaustive-deps

  const apply = useCallback(async () => {
    if (!activeChangeSet || !activeFile) return;
    setApplyState('applying');
    setApplyError(null);
    try {
      await applyAcceptedHunks(activeChangeSet.id, activeFile.filePath);
      setApplyState('idle');
    } catch (e) {
      setApplyState('error');
      setApplyError(e instanceof Error ? e.message : String(e));
    }
  }, [activeChangeSet, activeFile, applyAcceptedHunks]);

  const { lines, hunks } = useMemo(
    () => computeDiff(activeFile?.oldContent ?? '', activeFile?.newContent ?? ''),
    [activeFile?.oldContent, activeFile?.newContent],
  );

  const acceptAll = useCallback(async () => {
    if (!activeChangeSet || !activeFile) return;
    for (const h of hunks) setHunkDecision(activeChangeSet.id, activeFile.filePath, h.id, 'accepted');
    await apply();
  }, [activeChangeSet, activeFile, hunks, setHunkDecision, apply]);

  const rejectAll = useCallback(() => {
    if (!activeChangeSet || !activeFile) return;
    for (const h of hunks) setHunkDecision(activeChangeSet.id, activeFile.filePath, h.id, 'rejected');
  }, [activeChangeSet, activeFile, hunks, setHunkDecision]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!activeFile || !activeChangeSet) return;
      if (e.ctrlKey && e.key === 'Enter') {
        e.preventDefault();
        void acceptAll();
      }
      if (e.ctrlKey && e.key === 'Backspace') {
        e.preventDefault();
        rejectAll();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [activeFile, activeChangeSet, acceptAll, rejectAll]);

  if (!activeFile || !activeChangeSet) {
    return (
      <div className="flex-1 flex items-center justify-center bg-bg-primary">
        <div className="text-center space-y-3">
          <div className="text-4xl text-text-secondary opacity-20">
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1" strokeLinecap="round">
              <path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z" />
            </svg>
          </div>
          <p className="text-text-secondary text-sm">Select a file change to review</p>
          <p className="text-text-secondary text-xs">
            Click a file in the Source Control sidebar to preview its patch
          </p>
        </div>
      </div>
    );
  }

  const decisions = activeFile.hunkDecisions;
  const pendingHunks = hunks.filter((h) => (decisions[h.id] ?? 'pending') === 'pending').length;
  const acceptedHunks = hunks.filter((h) => decisions[h.id] === 'accepted').length;

  return (
    <div className="flex flex-col h-full bg-bg-primary">
      <Toolbar
        file={activeFile}
        hunkCount={hunks.length}
        pendingHunks={pendingHunks}
        acceptedHunks={acceptedHunks}
        applyState={applyState}
        applyError={applyError}
        onApply={apply}
        onAcceptAll={acceptAll}
        onRejectAll={rejectAll}
      />
      <div className="flex-1 overflow-auto font-mono text-xs leading-5">
        <DiffBody
          lines={lines}
          decisions={decisions}
          onAccept={(hunkId) => setHunkDecision(activeChangeSet.id, activeFile.filePath, hunkId, 'accepted')}
          onReject={(hunkId) => setHunkDecision(activeChangeSet.id, activeFile.filePath, hunkId, 'rejected')}
        />
      </div>
    </div>
  );
}

function Toolbar({
  file, hunkCount, pendingHunks, acceptedHunks, applyState, applyError, onApply, onAcceptAll, onRejectAll,
}: {
  file: FileChange;
  hunkCount: number;
  pendingHunks: number;
  acceptedHunks: number;
  applyState: 'idle' | 'applying' | 'error';
  applyError: string | null;
  onApply: () => void;
  onAcceptAll: () => void;
  onRejectAll: () => void;
}) {
  return (
    <div className="flex items-center justify-between px-3 py-2 bg-bg-secondary border-b border-border flex-shrink-0">
      <div className="flex items-center gap-2 text-xs min-w-0">
        <span className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${
          pendingHunks === 0 ? 'bg-green-400' : 'bg-yellow-400'
        }`} />
        <span className="text-text-secondary font-mono truncate">{file.filePath}</span>
        <span className="text-text-secondary opacity-50 flex-shrink-0">·</span>
        <span className="text-text-secondary flex-shrink-0">
          {hunkCount} hunk{hunkCount !== 1 ? 's' : ''}
          {pendingHunks > 0 && ` · ${pendingHunks} pending`}
          {acceptedHunks > 0 && ` · ${acceptedHunks} staged`}
        </span>
        {applyState === 'error' && (
          <span className="text-red-400 truncate" title={applyError ?? ''}>· apply failed</span>
        )}
      </div>
      <div className="flex items-center gap-2 flex-shrink-0">
        <button
          onClick={onRejectAll}
          className="px-2.5 py-1 text-xs rounded border border-red-700 text-red-400 hover:bg-red-900/30 transition-colors"
        >
          Reject All
        </button>
        <button
          onClick={onAcceptAll}
          className="px-2.5 py-1 text-xs rounded border border-green-700 text-green-400 hover:bg-green-900/30 transition-colors"
        >
          Accept All
        </button>
        <button
          onClick={onApply}
          disabled={applyState === 'applying' || acceptedHunks === 0}
          className="px-3 py-1 text-xs rounded bg-accent text-white hover:bg-accent/80 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
        >
          {applyState === 'applying' ? 'Applying…' : `Apply${acceptedHunks > 0 ? ` (${acceptedHunks})` : ''}`}
        </button>
      </div>
    </div>
  );
}

function DiffBody({
  lines, decisions, onAccept, onReject,
}: {
  lines: UnifiedLine[];
  decisions: Record<string, 'pending' | 'accepted' | 'rejected'>;
  onAccept: (hunkId: string) => void;
  onReject: (hunkId: string) => void;
}) {
  // Track which hunk ids have already shown their control row, so the ✓/✗
  // buttons render once per hunk (on its first line).
  const seen = new Set<string>();

  return (
    <table className="w-full border-collapse">
      <tbody>
        {lines.map((line, i) => {
          const decision = line.hunkId ? (decisions[line.hunkId] ?? 'pending') : null;
          const showControls = line.hunkId != null && !seen.has(line.hunkId);
          if (line.hunkId) seen.add(line.hunkId);

          const bg =
            line.type === 'add'
              ? decision === 'rejected' ? 'bg-bg-primary opacity-40' : 'bg-green-950/40'
              : line.type === 'del'
              ? decision === 'rejected' ? 'bg-bg-primary opacity-40' : 'bg-red-950/40'
              : '';
          const marker = line.type === 'add' ? '+' : line.type === 'del' ? '-' : ' ';
          const markerColor = line.type === 'add' ? 'text-green-400' : line.type === 'del' ? 'text-red-400' : 'text-text-secondary';

          return (
            <tr key={i} className={`${bg} group`}>
              {/* Gutter: per-hunk ✓/✗ controls (first line of each hunk) */}
              <td className="w-14 select-none align-top text-right pr-1 border-r border-border/40">
                {showControls && line.hunkId ? (
                  <HunkControls
                    decision={decision ?? 'pending'}
                    onAccept={() => onAccept(line.hunkId!)}
                    onReject={() => onReject(line.hunkId!)}
                  />
                ) : null}
              </td>
              {/* Old line number */}
              <td className="w-10 select-none text-right pr-2 text-text-secondary opacity-50 align-top">
                {line.oldLine ?? ''}
              </td>
              {/* New line number */}
              <td className="w-10 select-none text-right pr-2 text-text-secondary opacity-50 align-top">
                {line.newLine ?? ''}
              </td>
              {/* Marker */}
              <td className={`w-4 select-none text-center align-top ${markerColor}`}>{marker}</td>
              {/* Content */}
              <td className="whitespace-pre text-text-primary pl-1 pr-3">
                {line.text === '' ? ' ' : line.text}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

function HunkControls({
  decision, onAccept, onReject,
}: {
  decision: 'pending' | 'accepted' | 'rejected';
  onAccept: () => void;
  onReject: () => void;
}) {
  if (decision === 'accepted') {
    return (
      <button
        onClick={onReject}
        title="Staged — click to unstage"
        className="inline-flex items-center justify-center w-5 h-5 rounded text-green-400 hover:bg-green-900/40"
      >
        ✓
      </button>
    );
  }
  if (decision === 'rejected') {
    return (
      <button
        onClick={onAccept}
        title="Rejected — click to accept"
        className="inline-flex items-center justify-center w-5 h-5 rounded text-red-400 hover:bg-red-900/40"
      >
        ✗
      </button>
    );
  }
  return (
    <span className="inline-flex items-center gap-0.5 opacity-40 group-hover:opacity-100 transition-opacity">
      <button
        onClick={onAccept}
        title="Accept this hunk"
        className="inline-flex items-center justify-center w-4 h-4 rounded text-green-400 hover:bg-green-900/40"
      >
        ✓
      </button>
      <button
        onClick={onReject}
        title="Reject this hunk"
        className="inline-flex items-center justify-center w-4 h-4 rounded text-red-400 hover:bg-red-900/40"
      >
        ✗
      </button>
    </span>
  );
}
