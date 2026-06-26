/**
 * FileBrowser — multi-connection remote file tree view.
 *
 * Shows an independent file tree for every connected SSH machine.
 * Each connection section is collapsible. Disconnected machines are listed
 * but show a status placeholder instead of a tree.
 *
 * Pre-read: fileTreeCacheStore populates directory listings as soon as a
 * connection is ready, so trees render instantly.
 */
import { useState, useCallback, useRef, useEffect } from 'react';
import {
  ChevronRight,
  ChevronDown,
  FolderTree,
  WifiOff,
} from 'lucide-react';
import { useConnectionStore } from '../../stores/connectionStore';
import { useLayoutStore } from '../../stores/layoutStore';
import { useSessionStore } from '../../stores/sessionStore';
import { useFileTreeCacheStore } from '../../stores/fileTreeCacheStore';
import { ConnectionFileTree } from './ConnectionFileTree';
import type { ConnectionInfo } from '../../api/types';

/* ------------------------------------------------------------------ */
/*  Status helpers                                                     */
/* ------------------------------------------------------------------ */

function statusColor(status: ConnectionInfo['status']): string {
  switch (status) {
    case 'connected':
      return 'bg-green-500';
    case 'connecting':
    case 'bootstrapping':
    case 'reconnecting':
      return 'bg-yellow-500';
    case 'error':
      return 'bg-red-500';
    default:
      return 'bg-gray-500';
  }
}

/* ------------------------------------------------------------------ */
/*  ConnectionSection                                                  */
/* ------------------------------------------------------------------ */

function ConnectionSection({
  connection,
  expanded,
  onToggle,
  children,
}: {
  connection: ConnectionInfo;
  expanded: boolean;
  onToggle: () => void;
  children: React.ReactNode;
}) {
  const homeDir =
    connection.user === 'root' ? '/root' : `/home/${connection.user}`;

  return (
    <div className="border-b border-border last:border-b-0">
      {/* Section header */}
      <button
        onClick={onToggle}
        className="w-full flex items-center gap-2 px-3 py-2 hover:bg-bg-tertiary transition-colors text-left"
      >
        {expanded ? (
          <ChevronDown size={12} className="text-text-secondary flex-shrink-0" />
        ) : (
          <ChevronRight size={12} className="text-text-secondary flex-shrink-0" />
        )}
        <span className={`w-2 h-2 rounded-full flex-shrink-0 ${statusColor(connection.status)}`} />
        <span className="text-xs font-medium text-text-primary truncate">
          {connection.user}@{connection.host}
        </span>
        {connection.status === 'connected' && (
          <span className="text-[10px] text-text-secondary truncate font-mono ml-1">
            — {homeDir}
          </span>
        )}
        {connection.status !== 'connected' && connection.status !== 'disconnected' && (
          <span className="text-[10px] text-text-secondary ml-1 capitalize">
            {connection.status}…
          </span>
        )}
      </button>

      {/* Section body */}
      {expanded && <div className="pb-1">{children}</div>}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  DisconnectedPlaceholder                                            */
/* ------------------------------------------------------------------ */

function DisconnectedPlaceholder({ status }: { status: ConnectionInfo['status'] }) {
  const message =
    status === 'connecting'
      ? 'Connecting…'
      : status === 'bootstrapping'
      ? 'Bootstrapping agent…'
      : status === 'reconnecting'
      ? 'Reconnecting…'
      : status === 'error'
      ? 'Connection error'
      : 'Disconnected';

  return (
    <div className="flex flex-col items-center justify-center py-6 px-4 text-center">
      <WifiOff size={14} className="text-text-secondary opacity-40 mb-2" />
      <p className="text-[11px] text-text-secondary">{message}</p>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  FileBrowser                                                        */
/* ------------------------------------------------------------------ */

export function FileBrowser() {
  const connections = useConnectionStore((s) => s.connections);
  const addEditorTab = useLayoutStore((s) => s.addEditorTab);
  const fileTreeCache = useFileTreeCacheStore((s) => s.caches);

  // Active session work dir → focus in the matching connection tree.
  const activeSessionId = useSessionStore((s) => s.activeSessionId);
  const sessions = useSessionStore((s) => s.sessions);
  const activeSession = activeSessionId ? sessions[activeSessionId] : null;
  const activeWorkDir = activeSession?.metadata?.cwd || null;
  const activeSessionConnId = activeSession?.connectionId;

  // ── Dedup connections by host:port:user ───────────────────────────
  const statusRank: Record<string, number> = {
    connected: 10, bootstrapping: 8, connecting: 6,
    reconnecting: 4, disconnected: 2, error: 0,
  };

  // machineKey → { best connection, all connIds, user, host, homeDir }
  const machineMap = new Map<string, {
    key: string;
    conn: ConnectionInfo;
    connIds: string[];
    homeDir: string;
  }>();

  for (const conn of Object.values(connections)) {
    const key = `${conn.host}:${conn.port}:${conn.user}`;
    const homeDir = conn.user === 'root' ? '/root' : `/home/${conn.user}`;
    const existing = machineMap.get(key);
    if (existing) {
      existing.connIds.push(conn.id);
      if ((statusRank[conn.status] ?? 0) > (statusRank[existing.conn.status] ?? 0)) {
        existing.conn = conn;
      }
    } else {
      machineMap.set(key, { key, conn, connIds: [conn.id], homeDir });
    }
  }

  const machines = Array.from(machineMap.values());

  // ── Expand / collapse state (keyed by machine key) ────────────────
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());
  const seenConnectedRef = useRef(new Set<string>());

  useEffect(() => {
    for (const m of machines) {
      if (m.conn.status === 'connected' && !seenConnectedRef.current.has(m.key)) {
        seenConnectedRef.current.add(m.key);
        setExpandedIds((prev) => {
          if (prev.has(m.key)) return prev;
          return new Set(prev).add(m.key);
        });
      }
    }
  }, [machines]);

  const toggleSection = useCallback((key: string) => {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);

  const handleOpenFile = useCallback(
    (connectionId: string, path: string, name: string) => {
      addEditorTab({
        id: `file:${connectionId}:${path}`,
        filePath: path,
        label: name,
        connectionId,
      });
    },
    [addEditorTab],
  );

  // Which machine (if any) hosts the active session → show work dir focus.
  const focusMachineKey = activeSessionConnId
    ? machines.find((m) => m.connIds.includes(activeSessionConnId))?.key
    : null;

  /* -- render ---------------------------------------------------------- */

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-border">
        <FolderTree size={14} className="text-accent flex-shrink-0" />
        <span className="text-xs font-semibold text-text-secondary uppercase tracking-wider">
          Explorer
        </span>
      </div>

      {/* Connection list + trees */}
      <div className="flex-1 overflow-y-auto">
        {machines.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full px-4 text-center">
            <FolderTree size={24} className="text-text-secondary opacity-30 mb-2" />
            <p className="text-xs text-text-secondary">
              Connect to a remote machine to browse its files.
            </p>
          </div>
        ) : (
          machines.map((m) => {
            const expanded = expandedIds.has(m.key);
            const isFocused = m.key === focusMachineKey;

            return (
              <ConnectionSection
                key={m.key}
                connection={m.conn}
                expanded={expanded}
                onToggle={() => toggleSection(m.key)}
              >
                {m.conn.status === 'connected' ? (
                  <ConnectionFileTree
                    connectionId={m.conn.id}
                    homeDir={m.homeDir}
                    initialData={fileTreeCache[m.conn.id]}
                    focusPath={isFocused ? activeWorkDir : undefined}
                    onOpenFile={handleOpenFile}
                  />
                ) : (
                  <DisconnectedPlaceholder status={m.conn.status} />
                )}
              </ConnectionSection>
            );
          })
        )}
      </div>
    </div>
  );
}
