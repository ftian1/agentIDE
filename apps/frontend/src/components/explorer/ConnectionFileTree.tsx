/**
 * ConnectionFileTree — per-connection remote file tree.
 *
 * Manages one SSH connection's file tree with independent expand/collapse state,
 * git branch display, and connection-status awareness (clears tree on
 * disconnect, reloads on reconnect).
 *
 * Extracted from FileBrowser.tsx so the parent can render one instance per
 * connected machine.
 */
import { useState, useCallback, useEffect, useRef } from 'react';
import {
  ChevronRight,
  ChevronDown,
  FolderOpen,
  Folder,
  File,
  RefreshCw,
  CornerLeftUp,
  GitBranch,
  Check,
  WifiOff,
} from 'lucide-react';
import { useConnectionStore } from '../../stores/connectionStore';
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

export interface ConnectionFileTreeProps {
  connectionId: string;
  homeDir: string;
  /** Pre-read cache from fileTreeCacheStore — used for instant initial load. */
  initialData?: {
    rootPath: string;
    rootEntries: FileEntry[];
  };
  /** Current working directory to focus and highlight in the tree. */
  focusPath?: string | null;
  onOpenFile: (connectionId: string, path: string, name: string) => void;
}

/* ------------------------------------------------------------------ */
/*  FileTreeRow                                                        */
/* ------------------------------------------------------------------ */

function FileTreeRow({
  node,
  depth,
  onToggle,
  onOpenFile,
  focusedPath,
}: {
  node: TreeNode;
  depth: number;
  onToggle: (node: TreeNode) => void;
  onOpenFile: (node: TreeNode) => void;
  focusedPath?: string | null;
}) {
  const isDir = node.kind === 'directory';
  const expanded = !!(isDir && node.expanded);
  const isFocused = focusedPath != null && node.path === focusedPath;

  const handleClick = useCallback(() => {
    if (isDir) onToggle(node);
    else onOpenFile(node);
  }, [isDir, node, onToggle, onOpenFile]);

  const isParentNav = node.name === '..';

  return (
    <>
      <button
        onClick={handleClick}
        className={`w-full flex items-center gap-1 py-0.5 transition-colors text-left ${
          isFocused
            ? 'bg-accent/20 ring-1 ring-accent/40 hover:bg-accent/30'
            : 'hover:bg-bg-tertiary'
        }`}
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
        <span className={`text-xs truncate ${isParentNav ? 'text-accent' : isFocused ? 'text-accent font-medium' : 'text-text-primary'}`}>
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
              focusedPath={focusedPath}
            />
          ))}
        </>
      )}
    </>
  );
}

/* ------------------------------------------------------------------ */
/*  ConnectionFileTree                                                 */
/* ------------------------------------------------------------------ */

export function ConnectionFileTree({
  connectionId,
  homeDir,
  initialData,
  focusPath,
  onOpenFile,
}: ConnectionFileTreeProps) {
  const api = useTerminalApi();
  const connStatus = useConnectionStore((s) => s.connections[connectionId]?.status);

  const [tree, setTree] = useState<TreeNode[]>([]);
  const [initialLoaded, setInitialLoaded] = useState(false);

  // The directory git status is shown for.
  const [activeDir, setActiveDir] = useState<string | null>(null);
  const gitDir = activeDir ?? tree[0]?.path ?? null;

  // ── Work-dir focus / auto-expand ──────────────────────────────────
  const [focusedPath, setFocusedPath] = useState<string | null>(null);
  const treeRef = useRef(tree);
  treeRef.current = tree;

  useEffect(() => {
    if (!focusPath || !focusPath.startsWith(homeDir) || connStatus !== 'connected') {
      setFocusedPath(null);
      return;
    }
    let cancelled = false;

    const expandToPath = async () => {
      const rel = focusPath.slice(homeDir.length).split('/').filter(Boolean);
      if (rel.length === 0) {
        setFocusedPath(homeDir);
        return;
      }

      let currentPath = homeDir;

      for (const segment of rel) {
        if (cancelled) return;
        const targetPath = `${currentPath}/${segment}`;

        // Load children of targetPath (so we can navigate into it)
        try {
          const entries = await api.listFiles(connectionId, targetPath);
          if (cancelled) return;

          const parentPath =
            targetPath === '/' ? '/' : targetPath.split('/').slice(0, -1).join('/') || '/';
          setTree((prev) =>
            updateNode(prev, targetPath, (n) => ({
              ...n,
              children: [
                { name: '..', path: parentPath, kind: 'directory' as const },
                ...entries.map((e) => ({
                  name: e.name,
                  path: e.path,
                  kind: e.kind as 'file' | 'directory',
                })),
              ],
              loaded: true,
              expanded: true,
            })),
          );
        } catch (err) {
          console.error(`Failed to expand ${targetPath}:`, err);
          return;
        }

        currentPath = targetPath;
      }

      setFocusedPath(currentPath);
    };

    expandToPath();
    return () => { cancelled = true; };
  }, [focusPath, homeDir, connectionId, connStatus, api]);

  /* -- root loading ---------------------------------------------------- */

  useEffect(() => {
    if (connStatus !== 'connected') {
      setTree([]);
      setInitialLoaded(false);
      return;
    }
    if (initialLoaded) return;

    const loadRoot = async () => {
      try {
        let entries: FileEntry[];

        if (initialData && initialData.rootPath === homeDir) {
          // Use pre-read cache — instant
          entries = initialData.rootEntries;
        } else {
          entries = await api.listFiles(connectionId, homeDir);
        }

        const rootName = homeDir === '/' ? '/' : homeDir.split('/').pop() || homeDir;
        const node: TreeNode = {
          name: rootName,
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
        console.error(`Failed to load file tree for ${connectionId}:`, err);
      }
    };
    loadRoot();
  }, [connStatus, initialLoaded, connectionId, homeDir, initialData, api]);

  /* -- toggle directory ---------------------------------------------- */

  const handleToggle = useCallback(
    async (node: TreeNode) => {
      // Handle ".." — go to parent directory
      if (node.name === '..') {
        const parentPath = node.path;
        try {
          const entries = await api.listFiles(connectionId, parentPath);
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

      // Already loaded → just flip expand state
      if (node.loaded) {
        const willExpand = !node.expanded;
        if (willExpand) setActiveDir(node.path);
        setTree((prev) => updateNode(prev, node.path, (n) => ({
          ...n,
          expanded: willExpand,
        })));
        return;
      }

      // First expand → fetch children
      setActiveDir(node.path);
      setTree((prev) => updateNode(prev, node.path, (n) => ({ ...n, loading: true })));

      try {
        const entries = await api.listFiles(connectionId, node.path);
        const parentPath =
          node.path === '/' ? '/' : node.path.split('/').slice(0, -1).join('/') || '/';
        const children: TreeNode[] = [
          { name: '..', path: parentPath, kind: 'directory' },
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
          })),
        );
      } catch (err) {
        console.error('Failed to list directory:', err);
        setTree((prev) => updateNode(prev, node.path, (n) => ({ ...n, loading: false })));
      }
    },
    [connectionId, api],
  );

  /* -- open file ----------------------------------------------------- */

  const handleOpenFile = useCallback(
    (node: TreeNode) => {
      onOpenFile(connectionId, node.path, node.name);
    },
    [connectionId, onOpenFile],
  );

  /* -- render -------------------------------------------------------- */

  // Disconnected / error state
  if (connStatus !== 'connected') {
    return (
      <div className="flex flex-col items-center justify-center py-8 px-4 text-center">
        <WifiOff size={16} className="text-text-secondary opacity-40 mb-2" />
        <p className="text-[11px] text-text-secondary">
          {connStatus === 'reconnecting'
            ? 'Reconnecting…'
            : connStatus === 'error'
            ? 'Connection error'
            : 'Disconnected'}
        </p>
      </div>
    );
  }

  // Loading state
  if (tree.length === 0 && !initialLoaded) {
    return (
      <div className="flex flex-col items-center justify-center py-8 px-4 text-center">
        <RefreshCw size={14} className="text-accent animate-spin mb-2" />
        <p className="text-[11px] text-text-secondary">Loading file tree…</p>
      </div>
    );
  }

  return (
    <div>
      {/* Path bar with git branch + refresh */}
      <div className="flex items-center gap-2 px-3 py-1.5 bg-bg-tertiary/50">
        <p className="flex-1 text-[10px] text-text-secondary truncate min-w-0">
          <span className="text-text-primary font-mono">{tree[0]?.path ?? homeDir}</span>
        </p>
        {gitDir && (
          <GitBranchDropdown
            connectionId={connectionId}
            dir={gitDir}
            onChanged={() => setInitialLoaded(false)}
          />
        )}
        <button
          onClick={() => setInitialLoaded(false)}
          className="p-1 rounded text-text-secondary hover:text-text-primary hover:bg-bg-tertiary transition-colors flex-shrink-0"
          title="Refresh file tree"
        >
          <RefreshCw size={11} />
        </button>
      </div>

      {/* File tree */}
      <div className="py-1">
        {tree.map((node) => (
          <FileTreeRow
            key={node.path}
            node={node}
            depth={0}
            onToggle={handleToggle}
            onOpenFile={handleOpenFile}
            focusedPath={focusedPath}
          />
        ))}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Helpers                                                            */
/* ------------------------------------------------------------------ */

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

  useEffect(() => {
    let cancelled = false;
    fileApi
      .gitBranches(connectionId, dir)
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
      onChanged();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="relative min-w-0 flex-shrink-0" ref={ref}>
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
        <div
          className="absolute right-0 top-full mt-1 w-56 max-h-72 overflow-y-auto z-50
                        bg-bg-secondary border border-border rounded shadow-lg py-1"
        >
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
