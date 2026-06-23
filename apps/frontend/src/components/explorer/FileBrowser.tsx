/**
 * FileBrowser — remote file tree view.
 *
 * Connects to the remote machine via list_files and:
 *  - Starts at the user's home directory (/home/<user>)
 *  - Lazily loads directory contents on expand
 *  - Shows ".." entry for parent navigation
 *  - Sorts directories first, then files, alphabetically
 */
import { useState, useCallback, useEffect, useMemo } from 'react';
import {
  ChevronRight,
  ChevronDown,
  FolderOpen,
  Folder,
  File,
  FolderTree,
  RefreshCw,
  CornerLeftUp,
} from 'lucide-react';
import { useConnectionStore } from '../../stores/connectionStore';
import { useLayoutStore } from '../../stores/layoutStore';
import { useTerminalApi } from '../../hooks/useTerminalApi';
import type { FileEntry } from '../../api/terminalApi';

/* ------------------------------------------------------------------ */
/*  Types                                                              */
/* ------------------------------------------------------------------ */

interface TreeNode {
  name: string;
  path: string;
  kind: 'file' | 'directory';
  children?: TreeNode[];
  loaded?: boolean;   // true once children have been fetched
  loading?: boolean;  // true while fetch is in flight
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
  const expanded = isDir && node.loaded && (node.children?.length ?? 0) > 0;

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
          };
          setTree([parentNode]);
        } catch (err) {
          console.error('Failed to navigate to parent:', err);
        }
        return;
      }

      // If already loaded, just toggle (collapse by clearing children reference)
      if (node.loaded) {
        setTree((prev) => updateNode(prev, node.path, (n) => ({
          ...n,
          loaded: false,
          children: undefined,
        })));
        return;
      }

      // Mark loading
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
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-3 border-b border-border">
        <FolderTree size={14} className="text-accent" />
        <span className="text-xs font-semibold text-text-secondary uppercase tracking-wider">
          File Explorer
        </span>
      </div>

      {/* Connection info + current path */}
      {activeConn && connected && tree.length > 0 && (
        <div className="px-3 py-1.5 border-b border-border bg-bg-tertiary/50">
          <p className="text-[10px] text-text-secondary">
            <span className="text-accent">{activeConn.user}@{activeConn.host}</span>
            {' — '}
            <span className="text-text-primary font-mono">{tree[0].path}</span>
          </p>
        </div>
      )}

      {/* Refresh button */}
      {connected && (
        <div className="px-3 py-1 border-b border-border">
          <button
            onClick={() => { setInitialLoaded(false); }}
            className="flex items-center gap-1 text-[10px] text-text-secondary hover:text-text-primary transition-colors"
            title="Refresh file tree"
          >
            <RefreshCw size={10} />
            Refresh
          </button>
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
