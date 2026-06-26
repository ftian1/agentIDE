/**
 * AgentManagerPanel — two-level panel for connecting to remote machines and managing agents.
 *
 * Level 1: "Connect New Agent" button + list of connected agents grouped by machine,
 *          showing sessions under each agent. Disconnected machines have an inline
 *          Reconnect button that reconnects directly without navigating to the form.
 * Level 2: Connection form (host, port, password, agent type) with progress output.
 */
import { useState, useCallback, useRef, useEffect } from 'react';
import { ArrowLeft, Bot, ChevronDown, ChevronRight, Plus, Terminal, Wifi, WifiOff, X } from 'lucide-react';
import { listen } from '@tauri-apps/api/event';
import { useConnectionStore } from '../../stores/connectionStore';
import { useSessionStore } from '../../stores/sessionStore';
import { useLayoutStore } from '../../stores/layoutStore';
import { useAgentEngineStore, type AgentKind, AGENT_LABELS as AGENT_KIND_LABELS } from '../../stores/agentEngineStore';
import { useLlmProviderStore } from '../../stores/llmProviderStore';
import { buildSpawnEnv } from '../../lib/spawnEnv';
import type { ConnectionConfig } from '../../api/types';

/** In-memory cache of last-used credentials (survives page nav but not app restart). */
type CredentialCache = Map<string, ConnectionConfig>;

const LS_PREFIX = 'agentMgr';

function loadPassword(host: string, port: number, user: string): string | null {
  const key = `${LS_PREFIX}:pwd:${host}:${port}:${user}`;
  return localStorage.getItem(key);
}

function savePassword(host: string, port: number, user: string, password: string) {
  const key = `${LS_PREFIX}:pwd:${host}:${port}:${user}`;
  localStorage.setItem(key, password);
}

/** Main panel that switches between overview and connect-form views. */
export function AgentManagerPanel() {
  const [view, setView] = useState<'overview' | 'connect'>('overview');
  const [connectResult, setConnectResult] = useState<{ success: boolean; message: string } | null>(null);
  const [prefill, setPrefill] = useState<{ host: string; port: number; user: string; highlightPwd: boolean } | null>(null);
  const credentialsRef = useRef<CredentialCache>(new Map());
  const setOpenModal = useLayoutStore((s) => s.setOpenModal);

  const handleConnectSuccess = useCallback((message: string) => {
    setConnectResult({ success: true, message });
    setTimeout(() => {
      setView('overview');
      setConnectResult(null);
      setPrefill(null);
    }, 1500);
  }, []);

  const cacheCredential = useCallback((config: ConnectionConfig) => {
    const key = `${config.host}:${config.port}:${config.user}`;
    credentialsRef.current.set(key, config);
    if (config.password) {
      savePassword(config.host, config.port, config.user, config.password);
    }
  }, []);

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-3 border-b border-border">
        {view === 'connect' && (
          <button
            onClick={() => { setView('overview'); setConnectResult(null); }}
            className="text-text-secondary hover:text-text-primary transition-colors"
            title="Back"
          >
            <ArrowLeft size={14} />
          </button>
        )}
        <Bot size={14} className="text-accent" />
        <span className="text-xs font-semibold text-text-secondary uppercase tracking-wider">
          Agent Manager
        </span>
      </div>

      {connectResult && (
        <div
          className={`mx-2 mt-2 px-3 py-2 rounded text-xs ${
            connectResult.success
              ? 'bg-green-900/30 border border-green-700 text-green-300'
              : 'bg-red-900/30 border border-red-700 text-red-300'
          }`}
        >
          {connectResult.message}
        </div>
      )}

      {view === 'overview' ? (
        <OverviewView
          onAddAgent={() => setOpenModal('agentEngine')}
          onConnect={() => { setPrefill(null); setView('connect'); }}
          credentialsRef={credentialsRef}
        />
      ) : (
        <ConnectView
          onBack={() => { setView('overview'); setConnectResult(null); setPrefill(null); }}
          onSuccess={handleConnectSuccess}
          onCacheCredential={cacheCredential}
          prefill={prefill}
        />
      )}
    </div>
  );
}

/** Level 1 — agent overview: Connect button + per-machine agent/session list. */
function OverviewView({
  onAddAgent,
  onConnect,
  credentialsRef,
}: {
  onAddAgent: () => void;
  onConnect: () => void;
  credentialsRef: React.MutableRefObject<CredentialCache>;
}) {
  const connections = useConnectionStore((s) => s.connections);
  const sessions = useSessionStore((s) => s.sessions);
  const connList = Object.values(connections);
  const sessionList = Object.values(sessions);

  // Deduplicate by host:port:user
  const statusRank: Record<string, number> = {
    connected: 10, bootstrapping: 8, connecting: 6, reconnecting: 4, disconnected: 2, error: 0,
  };
  const machineMap = new Map<string, {
    key: string;
    host: string;
    port: number;
    user: string;
    label: string;
    bestConnId: string;
    bestStatus: string;
    connIds: string[];
  }>();
  for (const conn of connList) {
    const key = `${conn.host}:${conn.port}:${conn.user}`;
    const existing = machineMap.get(key);
    if (existing) {
      existing.connIds.push(conn.id);
      if ((statusRank[conn.status] ?? 0) > (statusRank[existing.bestStatus] ?? 0)) {
        existing.bestConnId = conn.id;
        existing.bestStatus = conn.status;
        existing.label = conn.label;
      }
    } else {
      machineMap.set(key, {
        key, host: conn.host, port: conn.port, user: conn.user,
        label: conn.label, bestConnId: conn.id, bestStatus: conn.status,
        connIds: [conn.id],
      });
    }
  }
  const machines = Array.from(machineMap.values());

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      <div className="px-3 py-3">
        <button
          onClick={onAddAgent}
          className="w-full flex items-center justify-center gap-2 px-3 py-2 rounded
                     bg-accent text-white text-xs font-medium
                     hover:bg-blue-500 transition-colors"
        >
          <Plus size={14} />
          Add Agent
        </button>
      </div>

      <div className="flex-1 overflow-y-auto px-2 pb-2">
        <p className="px-2 py-1 text-xs text-text-secondary uppercase tracking-wider">
          Connected Agents
        </p>

        {machines.length === 0 ? (
          <p className="px-2 py-3 text-xs text-text-secondary italic text-center">
            No agents connected. Click the button above to connect to a remote machine.
          </p>
        ) : (
          <div className="space-y-2">
            {machines.map((m) => {
              const machineSessions = sessionList.filter((s) => m.connIds.includes(s.connectionId));
              const credentialKey = `${m.host}:${m.port}:${m.user}`;
              const savedConfig = credentialsRef.current.get(credentialKey);
              return (
                <MachineCard
                  key={m.key}
                  host={m.host}
                  port={m.port}
                  user={m.user}
                  label={m.label}
                  connectionId={m.bestConnId}
                  status={m.bestStatus}
                  sessions={machineSessions}
                  savedPassword={savedConfig?.password ?? loadPassword(m.host, m.port, m.user)}
                  savedAuthMethod={savedConfig?.authMethod ?? 'password'}
                  savedAgentTool={(savedConfig as any)?.agentTool as AgentKind | undefined}
                  savedCliArgs={(savedConfig as any)?.cliArgs as string | undefined}
                  credentialsRef={credentialsRef}
                />
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

/** Card showing one machine. Supports inline reconnect with progress log. */
function MachineCard({
  host, port, user, label, connectionId, status, sessions,
  savedPassword, savedAuthMethod, savedAgentTool, savedCliArgs,
  credentialsRef,
}: {
  host: string;
  port: number;
  user: string;
  label: string;
  connectionId: string;
  status: string;
  sessions: Array<{ id: string; tool: string; state: string; pid?: number }>;
  savedPassword?: string | null;
  savedAuthMethod: string;
  savedAgentTool?: AgentKind;
  savedCliArgs?: string;
  credentialsRef: React.MutableRefObject<CredentialCache>;
}) {
  const [expanded, setExpanded] = useState(true);
  const [reconnecting, setReconnecting] = useState(false);
  const [reconnectLog, setReconnectLog] = useState<string[]>([]);
  const [reconnectError, setReconnectError] = useState<string | null>(null);
  const [showDisconnectConfirm, setShowDisconnectConfirm] = useState(false);
  const reconnectLogEndRef = useRef<HTMLDivElement>(null);

  // Auto-scroll reconnect log
  useEffect(() => {
    reconnectLogEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [reconnectLog]);

  const connect = useConnectionStore((s) => s.connect);
  const disconnect = useConnectionStore((s) => s.disconnect);
  const allConnections = useConnectionStore((s) => s.connections);
  const spawn = useSessionStore((s) => s.spawn);
  const closeSession = useSessionStore((s) => s.close);
  const setOpenModal = useLayoutStore((s) => s.setOpenModal);
  const setLastConn = useAgentEngineStore((s) => s.setLastConn);
  const agentConfigs = useAgentEngineStore((s) => s.configs);
  const llmProviders = useLlmProviderStore((s) => s.providers);
  const activeModel = useLlmProviderStore((s) => s.activeModel);
  const llmLoaded = useLlmProviderStore((s) => s.loaded);
  const llmLoad = useLlmProviderStore((s) => s.load);
  const bottomSessionId = useLayoutStore((s) => s.bottomPanelSessionId);

  // Ensure LLM providers are loaded (needed for __providers_json in spawn env).
  useEffect(() => {
    if (!llmLoaded) llmLoad();
  }, [llmLoaded, llmLoad]);

  const [deleteCountdown, setDeleteCountdown] = useState(0);
  const isConnected = status === 'connected' || status === 'bootstrapping';
  const isDisconnected = status === 'disconnected' || status === 'error';

  // Delete with 3-second countdown: first click starts countdown, second click confirms.
  const handleDeleteClick = useCallback(() => {
    if (deleteCountdown === 0) {
      setDeleteCountdown(3);
      const timer = setInterval(() => {
        setDeleteCountdown((prev) => {
          if (prev <= 1) { clearInterval(timer); return 0; }
          return prev - 1;
        });
      }, 1000);
      setTimeout(() => { clearInterval(timer); setDeleteCountdown(0); }, 3500);
      return;
    }
    // Actually delete — countdown is active, user clicked to confirm
    const ids = Object.values(allConnections)
      .filter((c) => c.host === host && c.port === port && c.user === user)
      .map((c) => c.id);
    for (const id of ids) {
      try { disconnect(id); } catch { /* ignore */ }
    }
    localStorage.removeItem(`${LS_PREFIX}:pwd:${host}:${port}:${user}`);
    setDeleteCountdown(0);
  }, [deleteCountdown, host, port, user, allConnections, disconnect]);
  const fullLabel = label || `${user}@${host}`;

  const appendLog = useCallback((msg: string) => {
    setReconnectLog((prev) => [...prev, `[${new Date().toLocaleTimeString()}] ${msg}`]);
  }, []);

  const handleReconnect = useCallback(async () => {
    if (!savedPassword && savedAuthMethod === 'password') {
      // Open Agent Engine Settings modal with pre-filled connection info
      setLastConn({ host, port, user, authMethod: 'password' });
      setOpenModal('agentEngine');
      return;
    }

    setReconnecting(true);
    setReconnectError(null);
    setReconnectLog([]);
    setExpanded(true);

    // Reset the bottom panel bash session so it auto-spawns against the new connection.
    useLayoutStore.getState().setBottomPanelSessionId(null);

    let unlisten: any = null;

    try {
      const config: ConnectionConfig = {
        host, port, user,
        authMethod: savedAuthMethod as ConnectionConfig['authMethod'],
        password: savedPassword ?? undefined,
      };
      appendLog(`Reconnecting to ${host}:${port} as ${user}...`);

      // Listen for real bootstrap progress events
      const bootstrapDone = new Promise<void>((resolve) => {
        listen<{ connection_id: string; phase: string; progress: number; message: string; error?: string }>(
          'bootstrap:progress',
          (event) => {
            const { phase, message, error: evtError } = event.payload;
            if (evtError) {
              appendLog(`✕ [${phase}] ${evtError}`);
            } else {
              appendLog(`[${phase}] ${message}`);
            }
            if (phase === 'complete') resolve();
          },
        ).then((fn) => { unlisten = fn; });
      });

      const connInfo = await connect(config);

      // Cache credential
      (config as any).agentTool = savedAgentTool ?? 'claude';
      const key = `${host}:${port}:${user}`;
      credentialsRef.current.set(key, config);

      // Wait for bootstrap to complete (with timeout)
      await Promise.race([
        bootstrapDone,
        new Promise<void>((_, reject) => setTimeout(() => reject(new Error('Bootstrap timeout')), 120000)),
      ]).catch(() => {});

      // Build env using shared logic — includes model vars, auth key, __providers_json.
      const tool = (savedAgentTool as AgentKind) ?? 'claude';
      const cfg = agentConfigs[tool] ?? { authKey: '', workDir: '', argPresets: [], extraArgs: '', envModels: {}, extraEnv: [] };
      const spawnEnv = buildSpawnEnv(tool, cfg, llmProviders, activeModel?.modelId);
      // Merge TMPDIR for the remote user.
      spawnEnv.TMPDIR = `/home/${user}/tmp`;
      spawnEnv.TMP = `/home/${user}/tmp`;
      spawnEnv.TEMP = `/home/${user}/tmp`;
      const args = (savedCliArgs ?? '').split(' ').filter(Boolean);
      appendLog(`Starting ${AGENT_KIND_LABELS[tool] ?? tool}...`);
      const session = await spawn(connInfo.id, {
        tool,
        args,
        cwd: undefined,
        env: Object.keys(spawnEnv).length > 0 ? spawnEnv : undefined,
      });
      appendLog(`✓ Agent started — session ${session.id.slice(0, 8)}`);

      setTimeout(() => {
        setReconnecting(false);
        // Keep the log visible — don't clear it.
      }, 1000);
    } catch (e) {
      const msg = String(e);
      appendLog(`✕ Error: ${msg}`);
      setReconnectError(msg);
      setReconnecting(false);
    } finally {
      unlisten?.();
    }
  }, [host, port, user, savedPassword, savedAuthMethod, savedAgentTool, savedCliArgs, connect, spawn, appendLog, credentialsRef]);

  return (
    <div className="rounded border border-border bg-bg-tertiary overflow-hidden">
      {/* Machine header */}
      <div className="flex items-stretch">
        {/* Chevron toggle (expand/collapse sessions) */}
        <button
          onClick={() => setExpanded(!expanded)}
          className="px-2 flex items-center text-text-secondary hover:text-text-primary transition-colors"
          title={expanded ? 'Collapse' : 'Expand'}
        >
          {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        </button>

        {/* Main click → open Agent Engine Settings modal with pre-filled config */}
        <button
          onClick={() => {
            setLastConn({ host, port, user, authMethod: savedAuthMethod as 'key' | 'password' | 'agent' });
            setOpenModal('agentEngine');
          }}
          className="flex-1 flex items-center gap-2 px-2 py-2 hover:bg-bg-primary/30 transition-colors text-left min-w-0"
        >
          {isConnected ? (
            <Wifi size={12} className="text-green-400 flex-shrink-0" />
          ) : (
            <WifiOff size={12} className="text-gray-500 flex-shrink-0" />
          )}
          <div className="flex-1 min-w-0">
            <span className="text-xs text-text-primary truncate block" title={fullLabel}>
              {fullLabel}
            </span>
            <span className="text-[10px] text-text-secondary truncate block" title={`${user}@${host}`}>
              {user}@{host}
            </span>
          </div>
          <span
            className={`text-[10px] px-1.5 py-0.5 rounded-full flex-shrink-0 ${
              isConnected
                ? 'bg-green-900/40 text-green-300'
                : 'bg-gray-700/40 text-gray-400'
            }`}
            title={status}
          >
            {status}
          </span>
        </button>

        {/* Inline Disconnect button */}
        {isConnected && (
          <button
            onClick={() => setShowDisconnectConfirm(true)}
            className="px-2.5 text-[10px] font-medium flex-shrink-0 border-l border-border
                       text-yellow-400 hover:bg-yellow-900/20 hover:text-yellow-300
                       transition-colors"
            title={`Disconnect from ${host}`}
          >
            ⏏
          </button>
        )}

        {/* Inline Reconnect button */}
        {isDisconnected && (
          <button
            onClick={handleReconnect}
            disabled={reconnecting}
            className={`px-2.5 text-[10px] font-medium flex-shrink-0 border-l border-border
                       transition-colors disabled:opacity-50 ${
              reconnecting
                ? 'text-yellow-400 bg-yellow-900/20'
                : 'text-accent hover:text-blue-300 hover:bg-bg-primary/30'
            }`}
            title={`Reconnect to ${host}`}
          >
            {reconnecting ? '...' : '↻'}
          </button>
        )}

        {/* Delete button — first click starts 3s countdown, second confirms */}
        <button
          onClick={handleDeleteClick}
          className={`w-7 flex items-center justify-center flex-shrink-0 border-l border-border transition-colors ${
            deleteCountdown > 0
              ? 'bg-red-900/30 text-red-400'
              : 'text-text-secondary hover:text-red-400 hover:bg-bg-primary/30'
          }`}
          title={deleteCountdown > 0 ? `Click again to confirm (${deleteCountdown}s)` : `Remove ${host}`}
        >
          {deleteCountdown > 0 ? (
            <span className="text-[10px] font-bold tabular-nums">{deleteCountdown}</span>
          ) : (
            <X size={12} />
          )}
        </button>
      </div>

      {/* Reconnect progress log */}
      {reconnectLog.length > 0 && (
        <div className="border-t border-border px-3 py-2">
          <div className="bg-bg-primary rounded p-2 max-h-32 overflow-y-auto font-mono text-[10px] leading-relaxed">
            {reconnectLog.map((line, i) => (
              <div
                key={i}
                className={
                  line.startsWith('✕') ? 'text-red-400' :
                  line.startsWith('✓') ? 'text-green-400' :
                  'text-text-secondary'
                }
              >
                {line}
              </div>
            ))}
            <div ref={reconnectLogEndRef} />
          </div>
          {reconnectError && !reconnecting && (
            <p className="text-[10px] text-red-400 mt-1">{reconnectError}</p>
          )}
        </div>
      )}

      {/* Sessions */}
      {expanded && (
        <div className="border-t border-border px-3 py-2 space-y-1">
          <p className="text-[10px] text-text-secondary uppercase tracking-wider mb-1">
            Sessions ({sessions.length})
          </p>
          {sessions.length === 0 ? (
            <p className="text-[10px] text-text-secondary italic">
              No sessions found on this machine.
            </p>
          ) : (
            sessions.map((s) => (
              <div
                key={s.id}
                className="flex items-center gap-2 px-2 py-1 rounded text-xs
                           bg-bg-primary/50 hover:bg-bg-primary transition-colors"
              >
                <span
                  className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${
                    s.state === 'running' ? 'bg-green-400' : s.state === 'spawning' ? 'bg-yellow-400' : 'bg-gray-500'
                  }`}
                  title={s.state}
                />
                <Terminal size={12} className="text-text-secondary flex-shrink-0" />
                <span className="text-text-primary truncate flex-1" title={s.tool}>{s.tool}</span>
                <span className="text-[10px] text-text-secondary flex-shrink-0" title={s.id}>#{s.id.slice(0, 6)}</span>
              </div>
            ))
          )}
        </div>
      )}

      {/* Disconnect confirmation modal */}
      {showDisconnectConfirm && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60" onClick={() => setShowDisconnectConfirm(false)}>
          <div className="bg-bg-primary border border-border rounded-lg shadow-xl w-80 p-4 space-y-3" onClick={(e) => e.stopPropagation()}>
            <p className="text-sm font-semibold text-text-primary">Disconnect from {host}?</p>
            <div className="text-xs text-text-secondary space-y-1">
              <p>This will:</p>
              <ul className="list-disc pl-4 space-y-0.5">
                <li>Close <span className="text-accent font-medium">{sessions.length} session(s)</span> in the editor</li>
                {bottomSessionId && <li>Close the bottom panel bash terminal</li>}
                <li>Terminate remote agent process</li>
              </ul>
            </div>
            <div className="flex gap-2 pt-1">
              <button
                onClick={() => setShowDisconnectConfirm(false)}
                className="flex-1 px-3 py-1.5 text-xs rounded border border-border text-text-secondary hover:text-text-primary transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={async () => {
                  setShowDisconnectConfirm(false);
                  for (const s of sessions) {
                    try { await closeSession(s.id); } catch { /* ignore */ }
                  }
                  if (bottomSessionId) {
                    try { await closeSession(bottomSessionId); } catch { /* ignore */ }
                    useLayoutStore.getState().setBottomPanelSessionId(null);
                  }
                  disconnect(connectionId);
                }}
                className="flex-1 px-3 py-1.5 text-xs rounded bg-red-600 text-white hover:bg-red-500 transition-colors font-medium"
              >
                Disconnect
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

/** Level 2 — connection form for a new agent. */
function ConnectView({
  onBack,
  onSuccess,
  onCacheCredential,
  prefill,
}: {
  onBack: () => void;
  onSuccess: (message: string) => void;
  onCacheCredential: (config: ConnectionConfig) => void;
  prefill?: { host: string; port: number; user: string; highlightPwd: boolean } | null;
}) {
  const isReconnect = !!prefill?.highlightPwd;
  const [host, setHost] = useState(() => (prefill?.host ?? localStorage.getItem('agentMgr:host')) ?? '');
  const [port, setPort] = useState(() => (prefill?.port ?? Number(localStorage.getItem('agentMgr:port'))) || 22);
  const [user, setUser] = useState(() => (prefill?.user ?? localStorage.getItem('agentMgr:user')) ?? 'root');
  const [password, setPassword] = useState('');
  const [agentTool, setAgentTool] = useState<AgentKind>(
    () => (localStorage.getItem('agentMgr:agentTool') as AgentKind) ?? 'claude',
  );
  const [useContainer, setUseContainer] = useState(
    () => localStorage.getItem('agentMgr:useContainer') === 'true',
  );
  const [containerName, setContainerName] = useState(
    () => localStorage.getItem('agentMgr:containerName') ?? '',
  );
  const [cliArgs, setCliArgs] = useState('');
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [log, setLog] = useState<string[]>([]);
  const logEndRef = useRef<HTMLDivElement>(null);

  const passwordRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (isReconnect && passwordRef.current) {
      passwordRef.current.focus();
    }
  }, [isReconnect]);

  const connect = useConnectionStore((s) => s.connect);
  const spawn = useSessionStore((s) => s.spawn);
  const agentConfigs = useAgentEngineStore((s) => s.configs);
  const llmProviders = useLlmProviderStore((s) => s.providers);
  const activeModel = useLlmProviderStore((s) => s.activeModel);
  const llmLoaded = useLlmProviderStore((s) => s.loaded);
  const llmLoad = useLlmProviderStore((s) => s.load);

  useEffect(() => {
    if (!llmLoaded) llmLoad();
  }, [llmLoaded, llmLoad]);

  const appendLog = useCallback((msg: string) => {
    setLog((prev) => [...prev, `[${new Date().toLocaleTimeString()}] ${msg}`]);
  }, []);

  // Auto-scroll to bottom when new log lines appear
  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [log]);

  const handleConnect = useCallback(async () => {
    if (!host || !user) return;
    localStorage.setItem('agentMgr:host', host);
    localStorage.setItem('agentMgr:port', String(port));
    localStorage.setItem('agentMgr:user', user);
    localStorage.setItem('agentMgr:agentTool', agentTool);
    localStorage.setItem('agentMgr:useContainer', String(useContainer));
    localStorage.setItem('agentMgr:containerName', containerName);
    localStorage.setItem('agentMgr:cliArgs', cliArgs);

    const config: ConnectionConfig = {
      host, port, user,
      authMethod: 'password',
      password,
    };
    // Cache in memory for later reconnect (include agentTool + args)
    (config as any).agentTool = agentTool;
    (config as any).cliArgs = cliArgs;
    onCacheCredential(config);

    setConnecting(true);
    setError(null);
    setLog([]);

    let unlisten: any = null;

    try {
      appendLog(`Connecting to ${host}:${port} as ${user}...`);

      // Listen for real bootstrap progress events
      const connIdPromise = new Promise<string>((resolve) => {
        listen<{ connection_id: string; phase: string; progress: number; message: string; error?: string }>(
          'bootstrap:progress',
          (event) => {
            const { connection_id, phase, message, error: evtError } = event.payload;
            if (evtError) {
              appendLog(`✕ [${phase}] ${evtError}`);
            } else {
              appendLog(`[${phase}] ${message}`);
            }
            if (phase === 'complete') {
              resolve(connection_id);
            }
          },
        ).then((fn) => { unlisten = fn; });
      });

      const connInfo = await connect(config);
      // Wait briefly for bootstrap:progress events to arrive, then proceed
      const timeout = new Promise<string>((_, reject) =>
        setTimeout(() => reject(new Error('Bootstrap timeout')), 120000),
      );
      await Promise.race([connIdPromise, timeout]).catch(() => {});

      if (useContainer && containerName) {
        appendLog(`Starting ${AGENT_KIND_LABELS[agentTool] ?? agentTool} in container ${containerName}...`);
      } else {
        appendLog(`Starting ${AGENT_KIND_LABELS[agentTool] ?? agentTool}...`);
      }
      const cfg = agentConfigs[agentTool] ?? { authKey: '', workDir: '', argPresets: [], extraArgs: '', envModels: {}, extraEnv: [] };
      const spawnEnv = buildSpawnEnv(agentTool, cfg, llmProviders, activeModel?.modelId);
      spawnEnv.TMPDIR = `/home/${user}/tmp`;
      spawnEnv.TMP = `/home/${user}/tmp`;
      spawnEnv.TEMP = `/home/${user}/tmp`;
      const session = await spawn(connInfo.id, {
        tool: agentTool,
        args: cliArgs.split(' ').filter(Boolean),
        cwd: undefined,
        env: Object.keys(spawnEnv).length > 0 ? spawnEnv : undefined,
        container: useContainer && containerName ? containerName : undefined,
      });
      appendLog(`Agent started — session ${session.id.slice(0, 8)}...`);

      appendLog('✓ Agent connected successfully!');
      onSuccess(`Agent ${AGENT_KIND_LABELS[agentTool] ?? agentTool} connected to ${host}`);
    } catch (e) {
      const msg = String(e);
      appendLog(`✕ Error: ${msg}`);
      setError(msg);
    } finally {
      unlisten?.();
      setConnecting(false);
    }
  }, [host, port, user, password, agentTool, useContainer, containerName, connect, spawn, appendLog, onSuccess, onCacheCredential]);

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      <div className="flex-1 overflow-y-auto px-3 py-3 space-y-3">
        {/* Host + Port */}
        <div className="grid grid-cols-3 gap-2">
          <div className="col-span-2">
            <label className="text-[10px] text-text-secondary block mb-1 uppercase tracking-wider">Machine Host</label>
            <input type="text" value={host} onChange={(e) => setHost(e.target.value)}
              onBlur={() => {
                const atIdx = host.indexOf('@');
                if (atIdx > 0 && (user === 'root' || !user)) {
                  setUser(host.slice(0, atIdx));
                  setHost(host.slice(atIdx + 1));
                }
              }}
              placeholder="e.g. 192.168.1.100 or user@host"
              className="w-full bg-bg-tertiary text-text-primary text-xs px-2 py-1.5 rounded border border-border focus:outline-none focus:border-accent placeholder:text-text-secondary" />
          </div>
          <div>
            <label className="text-[10px] text-text-secondary block mb-1 uppercase tracking-wider">Port</label>
            <input type="number" value={port} onChange={(e) => setPort(Number(e.target.value))}
              className="w-full bg-bg-tertiary text-text-primary text-xs px-2 py-1.5 rounded border border-border focus:outline-none focus:border-accent" />
          </div>
        </div>

        {/* Username */}
        <div>
          <label className="text-[10px] text-text-secondary block mb-1 uppercase tracking-wider">Username</label>
          <input type="text" value={user} onChange={(e) => setUser(e.target.value)} placeholder="e.g. root"
            className="w-full bg-bg-tertiary text-text-primary text-xs px-2 py-1.5 rounded border border-border focus:outline-none focus:border-accent placeholder:text-text-secondary" />
        </div>

        {/* Password */}
        <div>
          <label className="text-[10px] text-text-secondary block mb-1 uppercase tracking-wider">
            Password
            {isReconnect && (
              <span className="text-red-400 ml-1">— required to reconnect</span>
            )}
          </label>
          <input
            ref={passwordRef}
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder={isReconnect ? 'Enter password to reconnect' : 'SSH password'}
            className={`w-full bg-bg-tertiary text-text-primary text-xs px-2 py-1.5 rounded border
                       focus:outline-none focus:border-accent placeholder:text-text-secondary
                       ${isReconnect && !password ? 'field-highlight border-red-500' : 'border-border'}`}
          />
        </div>

        {/* Agent type */}
        <div>
          <label className="text-[10px] text-text-secondary block mb-1 uppercase tracking-wider">Agent CLI</label>
          <div className="flex gap-2">
            {(Object.entries(AGENT_KIND_LABELS) as [AgentKind, string][]).map(([tool, label]) => (
              <button key={tool} onClick={() => setAgentTool(tool)}
                className={`flex-1 px-3 py-1.5 text-xs rounded border transition-colors ${
                  agentTool === tool ? 'border-accent bg-accent/20 text-text-primary' : 'border-border bg-bg-tertiary text-text-secondary hover:text-text-primary'
                }`}>{label}</button>
            ))}
          </div>
        </div>

        {/* CLI Arguments */}
        <div>
          <label className="text-[10px] text-text-secondary block mb-1 uppercase tracking-wider">
            CLI Arguments
          </label>
          <input type="text" value={cliArgs} onChange={(e) => setCliArgs(e.target.value)}
            placeholder="e.g. --verbose"
            className="w-full bg-bg-tertiary text-text-primary text-xs px-2 py-1.5 rounded border border-border focus:outline-none focus:border-accent placeholder:text-text-secondary" />
        </div>

        {/* Container */}
        <div>
          <button type="button" onClick={() => setUseContainer(!useContainer)}
            className="flex items-center gap-1.5 text-[10px] text-text-secondary uppercase tracking-wider hover:text-text-primary transition-colors w-full text-left">
            <span className={`w-3 h-3 rounded border border-border flex items-center justify-center flex-shrink-0 ${useContainer ? 'bg-accent border-accent' : ''}`}>
              {useContainer && <span className="text-white text-[8px] leading-none">✓</span>}
            </span>
            Run in Container (optional)
          </button>
          {useContainer && (
            <div className="mt-2 p-2 rounded border border-border bg-bg-tertiary space-y-2">
              <div>
                <label className="text-[10px] text-text-secondary block mb-1 uppercase tracking-wider">Container Name / ID</label>
                <input type="text" value={containerName} onChange={(e) => setContainerName(e.target.value)} placeholder="e.g. dev-env"
                  className="w-full bg-bg-primary text-text-primary text-xs px-2 py-1.5 rounded border border-border focus:outline-none focus:border-accent placeholder:text-text-secondary" />
              </div>
              <p className="text-[10px] text-text-secondary opacity-70 leading-relaxed">
                The CLI will run via <code className="text-accent">docker exec -it &lt;container&gt; claude</code>.
              </p>
            </div>
          )}
        </div>

        {error && (
          <div className="p-2 rounded bg-red-900/30 border border-red-700 text-red-300 text-xs">{error}</div>
        )}

        <button onClick={handleConnect} disabled={connecting || !host || !user || !password}
          className="w-full px-3 py-2 text-xs font-medium rounded bg-accent text-white hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors">
          {connecting ? 'Connecting...' : 'Confirm & Connect'}
        </button>

        {log.length > 0 && (
          <div className="mt-3">
            <label className="text-[10px] text-text-secondary block mb-1 uppercase tracking-wider">Connection Output</label>
            <div className="bg-bg-tertiary border border-border rounded p-2 max-h-48 overflow-y-auto font-mono text-[11px] leading-relaxed">
              {log.map((line, i) => (
                <div key={i} className={line.startsWith('✕') ? 'text-red-400' : line.startsWith('✓') ? 'text-green-400' : 'text-text-secondary'}>{line}</div>
              ))}
              <div ref={logEndRef} />
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
