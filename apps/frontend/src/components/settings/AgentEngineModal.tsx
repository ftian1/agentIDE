/**
 * AgentEngineModal — two-tab "Agent Engine Settings" dialog.
 *
 * Tab 1 (Connection): SSH host/port/user/password + agent selection → Continue.
 * Tab 2 (Agent Setting): per-agent config. Claude gets work dir + launch-arg
 * presets + model env-var dropdowns sourced from configured LLM providers;
 * other agents get a generic work dir + free-text args + key/value env.
 *
 * Launch connects (if needed), spawns the session, and persists the config.
 */
import { useEffect, useMemo, useState } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import { useConnectionStore } from '../../stores/connectionStore';
import { useSessionStore } from '../../stores/sessionStore';
import { useLayoutStore } from '../../stores/layoutStore';
import { useLlmProviderStore } from '../../stores/llmProviderStore';
import { log } from '../../lib/debugLog';
import {
  useAgentEngineStore,
  type AgentKind,
  AGENT_LABELS,
} from '../../stores/agentEngineStore';

interface Props {
  onClose: () => void;
}

const inputCls =
  'w-full bg-bg-tertiary text-text-primary text-sm px-2 py-1.5 rounded border border-border ' +
  'focus:outline-none focus:border-accent placeholder:text-text-secondary';
const labelCls = 'text-xs text-text-secondary block mb-1';

const AGENTS: { kind: AgentKind; label: string; tool: string }[] = [
  { kind: 'claude', label: 'Claude Code CLI', tool: 'claude' },
  { kind: 'opencode', label: 'OpenCode', tool: 'opencode' },
  { kind: 'codex', label: 'Codex', tool: 'codex' },
  { kind: 'hermes', label: 'Hermes', tool: 'hermes' },
];

/** Curated `claude` launch flags (reference: claude --help). */
const CLAUDE_ARG_PRESETS: { id: string; label: string; tokens: string[] }[] = [
  { id: 'continue', label: 'Continue last session (--continue)', tokens: ['--continue'] },
  { id: 'verbose', label: 'Verbose logging (--verbose)', tokens: ['--verbose'] },
  {
    id: 'skip-permissions',
    label: 'Skip permission prompts (--dangerously-skip-permissions)',
    tokens: ['--dangerously-skip-permissions'],
  },
  { id: 'plan-mode', label: 'Plan mode (--permission-mode plan)', tokens: ['--permission-mode', 'plan'] },
  { id: 'no-cache', label: 'Disable prompt cache (--no-cache)', tokens: ['--no-cache'] },
];

/** Claude model-selecting env vars rendered as dropdowns. */
const CLAUDE_MODEL_ENV: { key: string; label: string }[] = [
  { key: 'ANTHROPIC_MODEL', label: 'ANTHROPIC_MODEL (primary)' },
  { key: 'ANTHROPIC_DEFAULT_OPUS_MODEL', label: 'ANTHROPIC_DEFAULT_OPUS_MODEL' },
  { key: 'ANTHROPIC_DEFAULT_SONNET_MODEL', label: 'ANTHROPIC_DEFAULT_SONNET_MODEL' },
  { key: 'ANTHROPIC_DEFAULT_HAIKU_MODEL', label: 'ANTHROPIC_DEFAULT_HAIKU_MODEL' },
  { key: 'CLAUDE_CODE_SUBAGENT_MODEL', label: 'CLAUDE_CODE_SUBAGENT_MODEL' },
];

type TabId = 'connection' | 'agent';

export function AgentEngineModal({ onClose }: Props) {
  const connect = useConnectionStore((s) => s.connect);
  const activeConnectionId = useConnectionStore((s) => s.activeConnectionId);
  const spawn = useSessionStore((s) => s.spawn);
  const setOpenModal = useLayoutStore((s) => s.setOpenModal);

  const providers = useLlmProviderStore((s) => s.providers);
  const activeModel = useLlmProviderStore((s) => s.activeModel);
  const loadProviders = useLlmProviderStore((s) => s.load);
  const providersLoaded = useLlmProviderStore((s) => s.loaded);

  const configs = useAgentEngineStore((s) => s.configs);
  const lastConn = useAgentEngineStore((s) => s.lastConn);
  const setConfig = useAgentEngineStore((s) => s.setConfig);
  const setLastConn = useAgentEngineStore((s) => s.setLastConn);
  const addProfile = useAgentEngineStore((s) => s.addProfile);

  const [tab, setTab] = useState<TabId>('connection');
  const [agent, setAgent] = useState<AgentKind>('claude');
  const [showErrors, setShowErrors] = useState(false);

  // Connection form (seeded from last-used).
  const [host, setHost] = useState(lastConn.host);
  const [port, setPort] = useState(lastConn.port);
  const [user, setUser] = useState(lastConn.user);
  const [authMethod, setAuthMethod] = useState(lastConn.authMethod);
  const [password, setPassword] = useState('');

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!providersLoaded) loadProviders();
  }, [providersLoaded, loadProviders]);

  // Flattened model options from all configured providers.
  const modelOptions = useMemo(
    () =>
      providers.flatMap((p) =>
        p.models.map((m) => ({ providerLabel: p.label, modelId: m.id })),
      ),
    [providers],
  );
  const hasModels = modelOptions.length > 0;

  const cfg = configs[agent];
  const meta = AGENTS.find((a) => a.kind === agent)!;

  const handleContinue = () => {
    const missing = !host.trim() || !user.trim()
      || (authMethod === 'password' && !password);
    if (missing) {
      setShowErrors(true);
      return;
    }
    setShowErrors(false);
    setError(null);
    setLastConn({ host, port, user, authMethod });
    setTab('agent');
  };

  const handleLaunch = async () => {
    setBusy(true);
    setError(null);
    try {
      // Connect if there's no active connection.
      let connId = activeConnectionId;
      if (!connId) {
        const info = await connect({
          host,
          port,
          user,
          authMethod,
          password: authMethod === 'password' ? password : undefined,
        });
        connId = info.id;
      }

      // Build args.
      const args: string[] = [];
      if (agent === 'claude') {
        for (const id of cfg.argPresets) {
          const preset = CLAUDE_ARG_PRESETS.find((p) => p.id === id);
          if (preset) args.push(...preset.tokens);
        }
      }
      if (cfg.extraArgs.trim()) {
        args.push(...cfg.extraArgs.trim().split(/\s+/));
      }

      // Build env.
      const env: Record<string, string> = {};
      if (agent === 'claude') {
        for (const [k, v] of Object.entries(cfg.envModels)) {
          if (v) env[k] = v;
        }
      }
      for (const { key, value } of cfg.extraEnv) {
        if (key.trim()) env[key.trim()] = value;
      }

      // Third-party provider routing (unified with tap proxy).
      const activeProvider = activeModel
        ? providers.find((p) => p.id === activeModel.providerId)
        : undefined;
      if (activeProvider?.kind === 'copilot' && activeProvider.copilotToken) {
        env.__gateway_provider = 'copilot';
        env.__gateway_token = activeProvider.copilotToken;
        env.__gateway_mode = 'passthrough';
      } else if (activeProvider && activeProvider.apiKey) {
        // Non-Copilot provider with API key → set ANTHROPIC env vars.
        // For OpenAI-compatible providers that support the Anthropic API
        // (e.g. DeepSeek /anthropic), route through the gateway so the
        // tap proxy can record traffic and inject auth.
        const base = (activeProvider.baseUrl || '').replace(/\/+$/, '');
        if (activeProvider.kind === 'deepseek') {
          // DeepSeek has an Anthropic-compatible Messages API at /anthropic
          env.__gateway_provider = 'deepseek';
          env.__gateway_token = activeProvider.apiKey;
          env.__gateway_mode = 'passthrough';
          env.ANTHROPIC_BASE_URL = `${base}/anthropic`;
        } else if (base) {
          env.ANTHROPIC_BASE_URL = base;
          env.ANTHROPIC_API_KEY = activeProvider.apiKey;
        } else {
          env.ANTHROPIC_API_KEY = activeProvider.apiKey;
        }
      }

      const launchEnv = Object.keys(env).length > 0 ? env : {};
      log('system', 'Launch env: ' + JSON.stringify(launchEnv));
      const sessionInfo = await spawn(connId, {
        tool: meta.tool,
        args,
        cwd: cfg.workDir || undefined,
        env: Object.keys(env).length > 0 ? env : undefined,
      });

      // Auto-save as a profile for the Agent Manager sidebar.
      addProfile({
        name: `${AGENT_LABELS[agent] ?? agent} on ${host}`,
        kind: agent,
        connectionId: connId,
        lastConn: { host, port, user, authMethod },
        config: cfg,
      });

      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
      <div className="bg-bg-secondary border border-border rounded-lg w-[600px] max-h-[85vh] shadow-2xl flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-border">
          <h2 className="text-sm font-semibold">Agent Engine Settings</h2>
          <button onClick={onClose} className="text-text-secondary hover:text-text-primary">
            ✕
          </button>
        </div>

        {/* Tabs */}
        <div className="flex items-center gap-1 px-4 pt-3 border-b border-border">
          {([
            { id: 'connection', label: '1 · SSH Connection' },
            { id: 'agent', label: '2 · Agent Setting' },
          ] as { id: TabId; label: string }[]).map((t) => (
            <button
              key={t.id}
              onClick={() => t.id === 'connection' && setTab('connection')}
              className={`px-3 py-1.5 text-xs transition-colors border-b-2 -mb-px ${
                tab === t.id
                  ? 'text-text-primary border-accent'
                  : 'text-text-secondary border-transparent hover:text-text-primary'
              } ${t.id === 'agent' ? 'cursor-default' : ''}`}
            >
              {t.label}
            </button>
          ))}
        </div>

        {/* Body */}
        <div className="p-4 space-y-3 overflow-y-auto flex-1">
          {tab === 'connection' ? (
            <ConnectionTab
              host={host} setHost={setHost}
              port={port} setPort={setPort}
              user={user} setUser={setUser}
              authMethod={authMethod} setAuthMethod={setAuthMethod}
              password={password} setPassword={setPassword}
              agent={agent} setAgent={setAgent}
              showErrors={showErrors}
            />
          ) : agent === 'claude' ? (
            <ClaudeAgentTab
              cfg={cfg}
              setConfig={(patch) => setConfig('claude', patch)}
              modelOptions={modelOptions}
              hasModels={hasModels}
              onConfigureProviders={() => setOpenModal('llmProviders')}
            />
          ) : (
            <GenericAgentTab
              label={meta.label}
              cfg={cfg}
              setConfig={(patch) => setConfig(agent, patch)}
            />
          )}

          {error && (
            <div className="p-2 rounded bg-red-900/30 border border-red-700 text-red-300 text-xs">
              {error}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex justify-between gap-2 px-4 py-3 border-t border-border">
          <div>
            {tab === 'agent' && (
              <button
                onClick={() => { setTab('connection'); setShowErrors(false); }}
                className="px-3 py-1.5 text-xs rounded bg-bg-tertiary hover:bg-border text-text-secondary"
              >
                ← Back
              </button>
            )}
          </div>
          <div className="flex gap-2">
            <button
              onClick={onClose}
              className="px-3 py-1.5 text-xs rounded bg-bg-tertiary hover:bg-border text-text-secondary"
            >
              Cancel
            </button>
            {tab === 'connection' ? (
              <button
                onClick={handleContinue}
                disabled={!host.trim() || !user.trim() || (authMethod === 'password' && !password)}
                className="px-4 py-1.5 text-xs rounded bg-accent text-white hover:bg-blue-500 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
              >
                Continue →
              </button>
            ) : (
              <button
                onClick={handleLaunch}
                disabled={busy}
                className="px-4 py-1.5 text-xs rounded bg-accent text-white hover:bg-blue-500 transition-colors disabled:opacity-50 flex items-center gap-1.5"
              >
                {busy && (
                  <span className="w-3 h-3 border-2 border-white/40 border-t-white rounded-full animate-spin" />
                )}
                Launch
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

/* ── Tab 1: connection ─────────────────────────────────────── */

interface ConnectionTabProps {
  host: string; setHost: (v: string) => void;
  port: number; setPort: (v: number) => void;
  user: string; setUser: (v: string) => void;
  authMethod: 'key' | 'password' | 'agent'; setAuthMethod: (v: 'key' | 'password' | 'agent') => void;
  password: string; setPassword: (v: string) => void;
  agent: AgentKind; setAgent: (v: AgentKind) => void;
  /** Set to true after the first "Continue" click to show validation errors. */
  showErrors: boolean;
}

function ConnectionTab(p: ConnectionTabProps) {
  const missingHost = p.showErrors && !p.host.trim();
  const missingUser = p.showErrors && !p.user.trim();
  const missingPwd = p.showErrors && p.authMethod === 'password' && !p.password;

  const errCls = 'border-red-500 focus:border-red-400';
  const normCls = 'focus:border-accent';

  function inputCls(missing: boolean): string {
    return `w-full bg-bg-tertiary text-text-primary text-sm px-2 py-1.5 rounded border ${
      missing ? errCls : 'border-border ' + normCls
    } focus:outline-none placeholder:text-text-secondary`;
  }
  const handleHostChange = (value: string) => {
    // Auto-extract user@host → username + hostname
    const atIdx = value.lastIndexOf('@');
    if (atIdx > 0 && !value.includes('://') && value.indexOf('@') === atIdx) {
      const extractedUser = value.slice(0, atIdx);
      const extractedHost = value.slice(atIdx + 1);
      if (extractedUser && extractedHost) {
        p.setUser(extractedUser);
        p.setHost(extractedHost);
        return;
      }
    }
    p.setHost(value);
  };
  return (
    <>
      <div className="grid grid-cols-3 gap-3">
        <div className="col-span-2">
          <label className={labelCls}>Host *</label>
          <input className={inputCls(missingHost)} value={p.host} onChange={(e) => handleHostChange(e.target.value)} placeholder="e.g. user@192.168.1.100" />
          {missingHost && <p className="text-[10px] text-red-400 mt-0.5">Host is required.</p>}
        </div>
        <div>
          <label className={labelCls}>Port</label>
          <input type="number" className={inputCls(false)} value={p.port} onChange={(e) => p.setPort(Number(e.target.value))} />
        </div>
      </div>

      <div>
        <label className={labelCls}>Username *</label>
        <input className={inputCls(missingUser)} value={p.user} onChange={(e) => p.setUser(e.target.value)} placeholder="e.g. root" />
        {missingUser && <p className="text-[10px] text-red-400 mt-0.5">Username is required.</p>}
      </div>

      <div>
        <label className={labelCls}>Auth Method</label>
        <div className="flex gap-2">
          {(['key', 'password', 'agent'] as const).map((m) => (
            <button
              key={m}
              onClick={() => p.setAuthMethod(m)}
              className={`px-3 py-1 text-xs rounded border transition-colors ${
                p.authMethod === m
                  ? 'border-accent bg-accent/20 text-text-primary'
                  : 'border-border bg-bg-tertiary text-text-secondary hover:text-text-primary'
              }`}
            >
              {m === 'key' ? 'SSH Key' : m === 'password' ? 'Password' : 'Agent'}
            </button>
          ))}
        </div>
      </div>

      {p.authMethod === 'password' && (
        <div>
          <label className={labelCls}>Password *</label>
          <input type="password" className={inputCls(missingPwd)} value={p.password} onChange={(e) => p.setPassword(e.target.value)} />
          {missingPwd && <p className="text-[10px] text-red-400 mt-0.5">Password is required for password auth.</p>}
        </div>
      )}

      <div>
        <label className={labelCls}>Agent</label>
        <select
          className={inputCls(false)}
          value={p.agent}
          onChange={(e) => p.setAgent(e.target.value as AgentKind)}
        >
          {AGENTS.map((a) => (
            <option key={a.kind} value={a.kind}>{a.label}</option>
          ))}
        </select>
      </div>
    </>
  );
}

/* ── Tab 2a: Claude ────────────────────────────────────────── */

interface ClaudeTabProps {
  cfg: import('../../stores/agentEngineStore').AgentEngineConfig;
  setConfig: (patch: Partial<import('../../stores/agentEngineStore').AgentEngineConfig>) => void;
  modelOptions: { providerLabel: string; modelId: string }[];
  hasModels: boolean;
  onConfigureProviders: () => void;
}

function ClaudeAgentTab({ cfg, setConfig, modelOptions, hasModels, onConfigureProviders }: ClaudeTabProps) {
  const togglePreset = (id: string) => {
    const next = cfg.argPresets.includes(id)
      ? cfg.argPresets.filter((x) => x !== id)
      : [...cfg.argPresets, id];
    setConfig({ argPresets: next });
  };

  return (
    <>
      <div>
        <label className={labelCls}>Work Directory</label>
        <input
          className={inputCls}
          value={cfg.workDir}
          onChange={(e) => setConfig({ workDir: e.target.value })}
          placeholder="/home/user/project"
        />
      </div>

      <div>
        <label className={labelCls}>Launch Arguments (multi-select)</label>
        <div className="flex flex-wrap gap-1.5">
          {CLAUDE_ARG_PRESETS.map((preset) => {
            const on = cfg.argPresets.includes(preset.id);
            return (
              <button
                key={preset.id}
                onClick={() => togglePreset(preset.id)}
                className={`px-2 py-1 text-[11px] rounded border transition-colors ${
                  on
                    ? 'border-accent bg-accent/20 text-text-primary'
                    : 'border-border bg-bg-tertiary text-text-secondary hover:text-text-primary'
                }`}
                title={preset.tokens.join(' ')}
              >
                {preset.label}
              </button>
            );
          })}
        </div>
        <input
          className={`${inputCls} mt-2`}
          value={cfg.extraArgs}
          onChange={(e) => setConfig({ extraArgs: e.target.value })}
          placeholder="Extra args (free text, e.g. --model opus)"
        />
      </div>

      <div>
        <label className={labelCls}>Model Environment Variables</label>
        {!hasModels ? (
          <div className="p-2.5 rounded bg-bg-tertiary border border-border text-xs text-text-secondary space-y-2">
            <p>No models available. Configure an LLM provider first.</p>
            <button
              onClick={onConfigureProviders}
              className="px-3 py-1 text-xs rounded bg-accent text-white hover:bg-blue-500 transition-colors"
            >
              Configure LLM Providers →
            </button>
          </div>
        ) : (
          <div className="space-y-2">
            {CLAUDE_MODEL_ENV.map((ev) => (
              <div key={ev.key} className="grid grid-cols-2 gap-2 items-center">
                <span className="text-xs text-text-secondary font-mono truncate" title={ev.key}>
                  {ev.key}
                </span>
                <select
                  className={inputCls}
                  value={cfg.envModels[ev.key] ?? ''}
                  onChange={(e) =>
                    setConfig({ envModels: { ...cfg.envModels, [ev.key]: e.target.value } })
                  }
                >
                  <option value="">— none —</option>
                  {modelOptions.map((m, i) => (
                    <option key={`${m.modelId}-${i}`} value={m.modelId}>
                      {m.modelId} ({m.providerLabel})
                    </option>
                  ))}
                </select>
              </div>
            ))}
          </div>
        )}
      </div>
    </>
  );
}

/* ── Tab 2b: generic agent ─────────────────────────────────── */

interface GenericTabProps {
  label: string;
  cfg: import('../../stores/agentEngineStore').AgentEngineConfig;
  setConfig: (patch: Partial<import('../../stores/agentEngineStore').AgentEngineConfig>) => void;
}

function GenericAgentTab({ label, cfg, setConfig }: GenericTabProps) {
  const addEnv = () => setConfig({ extraEnv: [...cfg.extraEnv, { key: '', value: '' }] });
  const updateEnv = (i: number, patch: Partial<{ key: string; value: string }>) => {
    const next = cfg.extraEnv.map((e, idx) => (idx === i ? { ...e, ...patch } : e));
    setConfig({ extraEnv: next });
  };
  const removeEnv = (i: number) =>
    setConfig({ extraEnv: cfg.extraEnv.filter((_, idx) => idx !== i) });

  return (
    <>
      <p className="text-xs text-text-secondary">{label} — generic configuration.</p>
      <div>
        <label className={labelCls}>Work Directory</label>
        <input
          className={inputCls}
          value={cfg.workDir}
          onChange={(e) => setConfig({ workDir: e.target.value })}
          placeholder="/home/user/project"
        />
      </div>
      <div>
        <label className={labelCls}>Launch Arguments</label>
        <input
          className={inputCls}
          value={cfg.extraArgs}
          onChange={(e) => setConfig({ extraArgs: e.target.value })}
          placeholder="Free text args, space-separated"
        />
      </div>
      <div>
        <div className="flex items-center justify-between mb-1">
          <label className={labelCls}>Environment Variables</label>
          <button
            onClick={addEnv}
            className="flex items-center gap-1 px-2 py-0.5 text-[11px] rounded bg-bg-tertiary hover:bg-border text-text-secondary hover:text-text-primary"
          >
            <Plus size={11} /> Add
          </button>
        </div>
        <div className="space-y-1.5">
          {cfg.extraEnv.length === 0 && (
            <p className="text-[11px] text-text-secondary italic">No environment variables.</p>
          )}
          {cfg.extraEnv.map((e, i) => (
            <div key={i} className="flex gap-1.5 items-center">
              <input
                className={inputCls}
                value={e.key}
                onChange={(ev) => updateEnv(i, { key: ev.target.value })}
                placeholder="KEY"
              />
              <input
                className={inputCls}
                value={e.value}
                onChange={(ev) => updateEnv(i, { value: ev.target.value })}
                placeholder="value"
              />
              <button
                onClick={() => removeEnv(i)}
                className="p-1.5 rounded text-text-secondary hover:text-red-300 hover:bg-bg-tertiary flex-shrink-0"
              >
                <Trash2 size={13} />
              </button>
            </div>
          ))}
        </div>
      </div>
    </>
  );
}
