/**
 * CodeChangesSidebar — file change list for Source Control activity.
 * Shows change sets with file trees, status indicators, and accept/reject buttons.
 */
import { FilePlus, FileEdit, FileX, CheckCircle, XCircle, Clock } from 'lucide-react';
import { useCodeChangeStore, type ChangeSet, type FileChange } from '../../stores/codeChangeStore';
import { useLayoutStore } from '../../stores/layoutStore';

export function CodeChangesSidebar() {
  const changeSets = useCodeChangeStore((s) => s.changeSets);
  const activeChangeSetId = useCodeChangeStore((s) => s.activeChangeSetId);
  const setActive = useCodeChangeStore((s) => s.setActiveChangeSet);
  const acceptAllInFile = useCodeChangeStore((s) => s.acceptAllInFile);
  const rejectAllInFile = useCodeChangeStore((s) => s.rejectAllInFile);
  const addTab = useLayoutStore((s) => s.addEditorTab);

  const sets = Object.values(changeSets);
  const pendingCount = sets.reduce(
    (sum, cs) =>
      sum + Object.values(cs.files).filter((f) => f.status === 'pending').length,
    0
  );

  const openFile = (cs: ChangeSet, file: FileChange) => {
    setActive(cs.id);
    addTab({
      id: file.id,
      filePath: file.filePath,
      label: file.filePath.split('/').pop() || file.filePath,
      icon: file.oldContent === null ? 'add' : file.newContent === null ? 'delete' : 'modify',
      changeSetId: cs.id,
    });
  };

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-3 border-b border-border">
        <span className="text-xs font-semibold text-text-secondary uppercase tracking-wider">
          Code Changes
        </span>
        {pendingCount > 0 && (
          <span className="text-[10px] px-1.5 py-0.5 rounded bg-accent/20 text-accent font-medium">
            {pendingCount} pending
          </span>
        )}
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto">
        {sets.length === 0 && (
          <div className="p-4 text-center">
            <p className="text-xs text-text-secondary italic">
              No code changes detected.
            </p>
            <p className="text-xs text-text-secondary mt-1 opacity-70">
              Changes from CLI sessions will appear here.
            </p>
          </div>
        )}

        {sets.map((cs) => (
          <div key={cs.id} className="border-b border-border last:border-b-0">
            {/* Change set header */}
            <div
              onClick={() => setActive(cs.id === activeChangeSetId ? null : cs.id)}
              className={`
                px-3 py-2 flex items-center gap-2 cursor-pointer transition-colors
                ${cs.id === activeChangeSetId ? 'bg-accent/10' : 'hover:bg-bg-tertiary'}
              `}
            >
              <span className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${
                cs.status === 'complete' ? 'bg-green-400' : 'bg-yellow-400'
              }`} />
              <span className="text-xs text-text-primary truncate flex-1">
                {cs.description || `Change Set ${cs.id.slice(0, 8)}`}
              </span>
              <span className="text-[10px] text-text-secondary">
                {Object.keys(cs.files).length} files
              </span>
            </div>

            {/* File list */}
            {cs.id === activeChangeSetId && (
              <div className="border-t border-border/50">
                {Object.entries(cs.files).map(([path, file]) => (
                  <FileRow
                    key={file.id}
                    file={file}
                    onClick={() => openFile(cs, file)}
                    onAccept={() => acceptAllInFile(cs.id, path)}
                    onReject={() => rejectAllInFile(cs.id, path)}
                  />
                ))}
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

function FileRow({
  file,
  onClick,
  onAccept,
  onReject,
}: {
  file: FileChange;
  onClick: () => void;
  onAccept: () => void;
  onReject: () => void;
}) {
  const icon = file.oldContent === null
    ? <FilePlus size={14} className="text-green-400 flex-shrink-0" />
    : file.newContent === null
    ? <FileX size={14} className="text-red-400 flex-shrink-0" />
    : <FileEdit size={14} className="text-yellow-400 flex-shrink-0" />;

  const statusIcon = file.status === 'accepted'
    ? <CheckCircle size={12} className="text-green-400 flex-shrink-0" />
    : file.status === 'rejected'
    ? <XCircle size={12} className="text-red-400 flex-shrink-0" />
    : <Clock size={12} className="text-text-secondary flex-shrink-0" />;

  const filename = file.filePath.split('/').pop() || file.filePath;

  return (
    <div
      className={`
        group px-3 py-1.5 flex items-center gap-2 cursor-pointer transition-colors
        hover:bg-bg-tertiary
        ${file.status === 'accepted' ? 'opacity-60' : ''}
        ${file.status === 'rejected' ? 'opacity-50' : ''}
      `}
    >
      <div onClick={onClick} className="flex items-center gap-2 flex-1 min-w-0">
        {icon}
        <span className="text-xs text-text-primary truncate">{filename}</span>
        <span className="text-[10px] text-text-secondary truncate opacity-70 hidden group-hover:inline">
          {file.filePath}
        </span>
      </div>
      {file.status === 'pending' && (
        <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
          <button
            onClick={(e) => { e.stopPropagation(); onReject(); }}
            className="p-0.5 hover:bg-red-900/30 rounded text-red-400"
            title="Reject"
          >
            <XCircle size={14} />
          </button>
          <button
            onClick={(e) => { e.stopPropagation(); onAccept(); }}
            className="p-0.5 hover:bg-green-900/30 rounded text-green-400"
            title="Accept"
          >
            <CheckCircle size={14} />
          </button>
        </div>
      )}
      {file.status !== 'pending' && statusIcon}
    </div>
  );
}
