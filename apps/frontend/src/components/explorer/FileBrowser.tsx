/**
 * FileBrowser — remote file tree view.
 *
 * Connects to the remote machine via list_files and:
 *  - Starts at the user's home directory (/home/<user>)
 *  - Lazily loads directory contents on expand
 *  - Shows ".." entry for parent navigation
 *  - Sorts directories first, then files, alphabetically
 */
import { useState, useCallback, useEffect, useRef } from 'react';
import {
  ChevronRight,
  ChevronDown,
  FolderOpen,
  Folder,
  File,
  FolderTree,
  RefreshCw,
  CornerLeftUp,
  GitBranch,
  Check,
} from 'lucide-react';
import { useConnectionStore } from '../../stores/connectionStore';
import { useLayoutStore } from '../../stores/layoutStore';
import { useTerminalApi } from '../../hooks/useTerminalApi';
import { createFileApi, type FileApi, type GitBranchInfo } from '../../api/fileApi';
import type { FileEntry } from '../../api/terminalApi';

const fileApi: FileApi = createFileApi();

/* ------------------------------------------------------------------ */
/*  Types                                                              */
/* ------------------------------------------------------------------ */

interface TreeNode {
  name: string;
  path: string;
  kind: 'file' | 'directory';
  children?: TreeNode[];
  loaded?: boolean;   // true once children have been fetched (cached)
  loading?: boolean;  // true while fetch is in flight
  expanded?: boolean; // UI expand state — decoupled from children cache so
                      // collapse/expand doesn't refetch.
}

/* ------------------------------------------------------------------ */
/*  FileTreeRow                                                        */
/* ------------------------------------------------------------------ */

function FileTreeRow({
  node,
  depth,
  onToggle,
  onOpenFile,
}: {
  node: TreeNode;
  depth: number;
  onToggle: (node: TreeNode) => void;
  onOpenFile: (node: TreeNode) => void;
}) {
  const isDir = node.kind === 'directory';
  const expanded = !!(isDir && node.expanded);

  const handleClick = useCallback(() => {
    if (isDir) onToggle(node);
    else onOpenFile(node);
  }, [isDir, node, onToggle, onOpenFile]);

  const isParentNav = node.name === '..';

  return (
    <>
      <button
        onClick={handleClick}
        className="w-full flex items-center gap-1 py-0.5 hover:bg-bg-tertiary transition-colors text-left"
        style={{ paddingLeft: 8 + depth * 16 }}
        title={node.path}
      >
        {/* Expand / collapse / loading chevron */}
        <span className="w-3.5 flex-shrink-0 flex items-center justify-center">
          {node.loading ? (
            <RefreshCw size={10} className="text-accent animate-spin" />
          ) : isDir ? (
            expanded ? (
              <ChevronDown size={12} className="text-text-secondary" />
            ) : (
              <ChevronRight size={12} className="text-text-secondary" />
            )
          ) : null}
        </span>

        {/* Icon */}
        {isParentNav ? (
          <CornerLeftUp size={14} className="text-accent flex-shrink-0" />
        ) : isDir ? (
          expanded ? (
            <FolderOpen size={14} className="text-yellow-500 flex-shrink-0" />
          ) : (
            <Folder size={14} className="text-yellow-500 flex-shrink-0" />
          )
        ) : (
          <File size={14} className="text-text-secondary flex-shrink-0" />
        )}

        {/* Name */}
        <span className={`text-xs truncate ${isParentNav ? 'text-accent' : 'text-text-primary'}`}>
          {node.name}
        </span>
      </button>

      {/* Children */}
      {isDir && expanded && node.children && (
        <>
          {node.children.map((child) => (
            <FileTreeRow
              key={child.path}
              node={child}
              depth={depth + 1}
              onToggle={onToggle}
              onOpenFile={onOpenFile}
            />
          ))}
        </>
      )}
    </>
  );
}

/* ------------------------------------------------------------------ */
/*  FileBrowser                                                        */
/* ------------------------------------------------------------------ */

export function FileBrowser() {
  const connections = useConnectionStore((s) => s.connections);
  const activeConnectionId = useConnectionStore((s) => s.activeConnectionId);
  const activeConn = activeConnectionId ? connections[activeConnectionId] : null;
  const addEditorTab = useLayoutStore((s) => s.addEditorTab);
  const api = useTerminalApi();

  const connected = activeConn?.status === 'connected';
  const homeDir = activeConn ? (activeConn.user === 'root' ? '/root' : `/home/${activeConn.user}`) : '/home';

  // Root of the tree — starts at home directory
  const [tree, setTree] = useState<TreeNode[]>([]);
  const [initialLoaded, setInitialLoaded] = useState(false);

  // The directory the tree is currently rooted at — used for git queries.
  const currentDir = tree[0]?.path ?? null;

  // Load initial directory on connect
  useEffect(() => {
    if (!connected || !activeConn) {
      setTree([]);
      setInitialLoaded(false);
      return;
    }
    if (initialLoaded) return;

    const loadRoot = async () => {
      try {
        const entries = await api.listFiles(activeConn.id, homeDir);
        const node: TreeNode = {
          name: homeDir === '/' ? '/' : homeDir.split('/').pop() || homeDir,
          path: homeDir,
          kind: 'directory',
          children: entries.map((e) => ({
            name: e.name,
            path: e.path,
            kind: e.kind as 'file' | 'directory',
          })),
          loaded: true,
          expanded: true,
        };
        setTree([node]);
        setInitialLoaded(true);
      } catch (err) {
        console.error('Failed to load file tree:', err);
      }
    };
    loadRoot();
  }, [connected, activeConn?.id, initialLoaded]);

  // Reset when connection changes
  useEffect(() => {
    setInitialLoaded(false);
    setTree([]);
  }, [activeConnectionId]);

  // Toggle directory: load children if not yet fetched
  const handleToggle = useCallback(
    async (node: TreeNode) => {
      if (!activeConn) return;

      // Handle ".." — go to parent directory
      if (node.name === '..') {
        const parentPath = node.path;
        try {
          const entries = await api.listFiles(activeConn.id, parentPath);
          const parentName = parentPath === '/' ? '/' : parentPath.split('/').pop() || parentPath;
          const parentNode: TreeNode = {
            name: parentName,
            path: parentPath,
            kind: 'directory',
            children: entries.map((e) => ({
              name: e.name,
              path: e.path,
              kind: e.kind as 'file' | 'directory',
            })),
            loaded: true,
            expanded: true,
          };
          setTree([parentNode]);
        } catch (err) {
          console.error('Failed to navigate to parent:', err);
        }
        return;
      }

      // Already loaded → just flip expand state. Children stay cached, so
      // re-expanding is instant (no refetch — that was the "refresh lag").
      if (node.loaded) {
        setTree((prev) => updateNode(prev, node.path, (n) => ({
          ...n,
          expanded: !n.expanded,
        })));
        return;
      }

      // First expand of this dir → fetch children, then expand.
      setTree((prev) => updateNode(prev, node.path, (n) => ({ ...n, loading: true })));

      try {
        const entries = await api.listFiles(activeConn.id, node.path);
        // Add ".." parent link at the top
        const parentPath = node.path === '/' ? '/' : node.path.split('/').slice(0, -1).join('/') || '/';
        const children: TreeNode[] = [
          {
            name: '..',
            path: parentPath,
            kind: 'directory',
          },
          ...entries.map((e) => ({
            name: e.name,
            path: e.path,
            kind: e.kind as 'file' | 'directory',
          })),
        ];
        setTree((prev) =>
          updateNode(prev, node.path, (n) => ({
            ...n,
            children,
            loaded: true,
            loading: false,
            expanded: true,
          }))
        );
      } catch (err) {
        console.error('Failed to list directory:', err);
        setTree((prev) => updateNode(prev, node.path, (n) => ({ ...n, loading: false })));
      }
    },
    [activeConn, api]
  );

  // Open a file into the center editor as a 'file' editor tab.
  const handleOpenFile = useCallback(
    (node: TreeNode) => {
      if (!activeConn) return;
      addEditorTab({
        id: `file:${activeConn.id}:${node.path}`,
        filePath: node.path,
        label: node.name,
        connectionId: activeConn.id,
      });
    },
    [activeConn, addEditorTab],
  );

  return (
    <div className="flex flex-col h-full">
      {/* Header: title + git branch dropdown + refresh icon, all on one row */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-border">
        <FolderTree size={14} className="text-accent flex-shrink-0" />
        <span className="text-xs font-semibold text-text-secondary uppercase tracking-wider flex-shrink-0">
          Explorer
        </span>
        <div className="flex-1 min-w-0 flex justify-end">
          {activeConn && connected && currentDir && (
            <GitBranchDropdown
              connectionId={activeConn.id}
              dir={currentDir}
              onChanged={() => setInitialLoaded(false)}
            />
          )}
        </div>
        {connected && (
          <button
            onClick={() => setInitialLoaded(false)}
            className="p-1 rounded text-text-secondary hover:text-text-primary hover:bg-bg-tertiary transition-colors flex-shrink-0"
            title="Refresh file tree"
          >
            <RefreshCw size={12} />
          </button>
        )}
      </div>

      {/* Connection info + current path */}
      {activeConn && connected && tree.length > 0 && (
        <div className="px-3 py-1.5 border-b border-border bg-bg-tertiary/50">
          <p className="text-[10px] text-text-secondary truncate">
            <span className="text-accent">{activeConn.user}@{activeConn.host}</span>
            {' — '}
            <span className="text-text-primary font-mono">{tree[0].path}</span>
          </p>
        </div>
      )}

      {/* File tree or empty state */}
      <div className="flex-1 overflow-y-auto py-1">
        {tree.length > 0 ? (
          tree.map((node) => (
            <FileTreeRow
              key={node.path}
              node={node}
              depth={0}
              onToggle={handleToggle}
              onOpenFile={handleOpenFile}
            />
          ))
        ) : (
          <div className="flex flex-col items-center justify-center h-full px-4 text-center">
            <FolderTree size={24} className="text-text-secondary opacity-30 mb-2" />
            <p className="text-xs text-text-secondary">
              {connected
                ? 'Loading file tree...'
                : 'Connect to a remote machine to browse its files.'}
            </p>
          </div>
        )}
      </div>
    </div>
  );
}

/** Immutable-ish update of a tree node by path. */
function updateNode(
  nodes: TreeNode[],
  targetPath: string,
  fn: (n: TreeNode) => TreeNode,
): TreeNode[] {
  return nodes.map((n) => {
    if (n.path === targetPath) return fn(n);
    if (n.children) {
      return { ...n, children: updateNode(n.children, targetPath, fn) };
    }
    return n;
  });
}

/* ------------------------------------------------------------------ */
/*  GitBranchDropdown                                                  */
/* ------------------------------------------------------------------ */

/**
 * Shows the current git branch of the explorer's root dir (if it's a repo) as
 * a dropdown that lets the user check out a different local branch. Renders
 * nothing when the directory isn't a git repository.
 */
function GitBranchDropdown({
  connectionId,
  dir,
  onChanged,
}: {
  connectionId: string;
  dir: string;
  onChanged: () => void;
}) {
  const [info, setInfo] = useState<GitBranchInfo | null>(null);
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  // (Re)load branch info whenever the root dir changes.
  useEffect(() => {
    let cancelled = false;
    fileApi.gitBranches(connectionId, dir)
      .then((i) => { if (!cancelled) setInfo(i); })
      .catch(() => { if (!cancelled) setInfo(null); });
    return () => { cancelled = true; };
  }, [connectionId, dir]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener('mousedown', onDown);
    return () => window.removeEventListener('mousedown', onDown);
  }, [open]);

  if (!info?.isRepo) return null;

  const checkout = async (branch: string) => {
    if (branch === info.current || busy) return;
    setBusy(true);
    setError(null);
    try {
      await fileApi.gitCheckout(connectionId, dir, branch);
      const next = await fileApi.gitBranches(connectionId, dir);
      setInfo(next);
      setOpen(false);
      onChanged(); // refresh the file tree against the new branch
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="relative min-w-0" ref={ref}>
      <button
        onClick={() => setOpen((v) => !v)}
        disabled={busy}
        className="flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] text-text-secondary
                   hover:text-text-primary hover:bg-bg-tertiary transition-colors max-w-[140px]"
        title={`Git branch: ${info.current} — click to switch`}
      >
        <GitBranch size={11} className="flex-shrink-0" />
        <span className="truncate">{busy ? '切换中…' : info.current || '(detached)'}</span>
        <ChevronDown size={10} className={`flex-shrink-0 transition-transform ${open ? 'rotate-180' : ''}`} />
      </button>

      {open && (
        <div className="absolute right-0 top-full mt-1 w-56 max-h-72 overflow-y-auto z-50
                        bg-bg-secondary border border-border rounded shadow-lg py-1">
          <div className="px-3 py-1 text-[10px] uppercase tracking-wider text-text-secondary border-b border-border/60">
            切换分支 · {info.branches.length}
          </div>
          {error && (
            <div className="px-3 py-1.5 text-[10px] text-red-400 break-words">{error}</div>
          )}
          {info.branches.map((b) => (
            <button
              key={b}
              onClick={() => checkout(b)}
              className="w-full flex items-center gap-2 px-3 py-1.5 text-xs text-left
                         text-text-primary hover:bg-bg-tertiary transition-colors"
            >
              <span className="w-3.5 flex-shrink-0">
                {b === info.current && <Check size={12} className="text-green-400" />}
              </span>
              <span className="truncate">{b}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
