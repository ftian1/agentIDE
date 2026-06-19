/**
 * CodeChangeEditor — main code review workspace.
 *
 * Renders a Monaco diff editor with syntax highlighting, file tabs,
 * and accept/reject controls. Supports keyboard shortcuts:
 *   Ctrl+Enter — accept current file
 *   Ctrl+Backspace — reject current file
 */
import { useCallback, useEffect } from 'react';
import { DiffEditor } from '@monaco-editor/react';
import { useCodeChangeStore } from '../../stores/codeChangeStore';
import { useLayoutStore } from '../../stores/layoutStore';
import { detectLanguage } from '../../lib/languageDetector';

export function CodeChangeEditor() {
  const changeSets = useCodeChangeStore((s) => s.changeSets);
  const activeChangeSetId = useCodeChangeStore((s) => s.activeChangeSetId);
  const acceptAllInFile = useCodeChangeStore((s) => s.acceptAllInFile);
  const rejectAllInFile = useCodeChangeStore((s) => s.rejectAllInFile);

  const editorTabs = useLayoutStore((s) => s.editorTabs);
  const activeTabId = useLayoutStore((s) => s.activeEditorTabId);
  const addTab = useLayoutStore((s) => s.addEditorTab);
  const removeTab = useLayoutStore((s) => s.removeEditorTab);
  const setActiveTab = useLayoutStore((s) => s.setActiveEditorTab);

  // Find active file from editor tab
  const activeTab = editorTabs.find((t) => t.id === activeTabId);
  const activeChangeSet = activeTab?.changeSetId
    ? changeSets[activeTab.changeSetId]
    : null;
  const activeFile = activeChangeSet && activeTab
    ? activeChangeSet.files[activeTab.filePath]
    : null;

  // Open files from active change set into editor tabs
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

  // Keyboard shortcuts
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!activeFile || !activeChangeSet) return;
      if (e.ctrlKey && e.key === 'Enter') {
        e.preventDefault();
        acceptAllInFile(activeChangeSet.id, activeFile.filePath);
      }
      if (e.ctrlKey && e.key === 'Backspace') {
        e.preventDefault();
        rejectAllInFile(activeChangeSet.id, activeFile.filePath);
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [activeFile, activeChangeSet, acceptAllInFile, rejectAllInFile]);

  if (!activeFile) {
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
            Click a file in the Source Control sidebar to view its diff
          </p>
        </div>
      </div>
    );
  }

  const lang = detectLanguage(activeFile.filePath);
  const isAccepted = activeFile.status === 'accepted';
  const isRejected = activeFile.status === 'rejected';

  return (
    <div className="flex flex-col h-full bg-bg-primary">
      {/* Toolbar */}
      <div className="flex items-center justify-between px-3 py-2 bg-bg-secondary border-b border-border flex-shrink-0">
        <div className="flex items-center gap-2 text-xs">
          <span className={`w-1.5 h-1.5 rounded-full ${
            isAccepted ? 'bg-green-400' : isRejected ? 'bg-red-400' : 'bg-yellow-400'
          }`} />
          <span className="text-text-secondary font-mono">{activeFile.filePath}</span>
          <span className="text-text-secondary opacity-50">·</span>
          <span className="text-text-secondary">{lang}</span>
        </div>
        <div className="flex items-center gap-2">
          {activeFile.status === 'pending' && (
            <>
              <button
                onClick={() => activeChangeSet && rejectAllInFile(activeChangeSet.id, activeFile.filePath)}
                className="px-3 py-1 text-xs rounded border border-red-700 text-red-400
                           hover:bg-red-900/30 transition-colors"
              >
                Reject File
              </button>
              <button
                onClick={() => activeChangeSet && acceptAllInFile(activeChangeSet.id, activeFile.filePath)}
                className="px-3 py-1 text-xs rounded bg-green-700 text-white
                           hover:bg-green-600 transition-colors"
              >
                Accept File
              </button>
            </>
          )}
          {isAccepted && (
            <span className="text-xs text-green-400 flex items-center gap-1">
              ✓ Accepted
            </span>
          )}
          {isRejected && (
            <span className="text-xs text-red-400 flex items-center gap-1">
              ✕ Rejected
            </span>
          )}
        </div>
      </div>

      {/* Monaco diff editor */}
      <div className="flex-1 overflow-hidden">
        <DiffEditor
          height="100%"
          original={activeFile.oldContent ?? ''}
          modified={activeFile.newContent ?? ''}
          language={lang}
          theme="vs-dark"
          options={{
            readOnly: true,
            renderSideBySide: true,
            minimap: { enabled: false },
            lineNumbers: 'on',
            scrollBeyondLastLine: false,
            fontSize: 13,
            fontFamily: "'Cascadia Code', 'JetBrains Mono', 'Fira Code', 'Consolas', monospace",
            renderOverviewRuler: false,
            diffWordWrap: 'on',
            padding: { top: 8, bottom: 8 },
          }}
          loading={
            <div className="flex items-center justify-center h-full">
              <p className="text-text-secondary text-sm">Loading diff editor...</p>
            </div>
          }
        />
      </div>
    </div>
  );
}
