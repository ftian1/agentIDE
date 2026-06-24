/**
 * LlmProviderModal — multi-provider LLM configuration dialog.
 *
 * Left column: provider list with status dots + "Add Provider".
 * Right column: per-provider config —
 *   · Copilot           → GitHub OAuth device-code flow (show code, poll token).
 *   · OpenAI-compatible → base URL + API key, auto-fetch /models (manual fallback).
 *
 * Mirrors AgentBackendModal styling. Persists via llmProviderStore.
 */
import { useEffect, useRef, useState } from 'react';
import { Plus, Trash2, Check, Copy, RefreshCw } from 'lucide-react';
import {
  useLlmProviderStore,
  type LlmProvider,
  type ProviderKind,
} from '../../stores/llmProviderStore';
import { createLlmApi } from '../../api/llmApi';

interface Props {
  onClose: () => void;
}

const api = createLlmApi();

const inputCls =
  'w-full bg-bg-tertiary text-text-primary text-sm px-2 py-1.5 rounded border border-border ' +
  'focus:outline-none focus:border-accent placeholder:text-text-secondary';
const labelCls = 'text-xs text-text-secondary block mb-1';

function statusDot(status: LlmProvider['status']): string {
  switch (status) {
    case 'authenticated':
    case 'key-set':
      return 'bg-green-500';
    case 'authenticating':
      return 'bg-yellow-500';
    case 'error':
      return 'bg-red-500';
    default:
      return 'bg-text-secondary/40';
  }
}

function statusLabel(p: LlmProvider): string {
  switch (p.status) {
    case 'authenticated':
      return p.kind === 'copilot' ? 'authenticated' : 'key set';
    case 'key-set':
      return 'key set';
    case 'authenticating':
      return 'authenticating…';
    case 'error':
      return p.error ? `error: ${p.error}` : 'error';
    default:
      return 'not configured';
  }
}

export function LlmProviderModal({ onClose }: Props) {
  const providers = useLlmProviderStore((s) => s.providers);
  const load = useLlmProviderStore((s) => s.load);
  const loaded = useLlmProviderStore((s) => s.loaded);
  const addProvider = useLlmProviderStore((s) => s.addProvider);
  const updateProvider = useLlmProviderStore((s) => s.updateProvider);
  const removeProvider = useLlmProviderStore((s) => s.removeProvider);
  const refreshModels = useLlmProviderStore((s) => s.refreshModels);

  const [selectedId, setSelectedId] = useState<string | null>(null);

  useEffect(() => {
    if (!loaded) load();
  }, [loaded, load]);

  // Keep a valid selection.
  useEffect(() => {
    if (providers.length === 0) {
      setSelectedId(null);
    } else if (!providers.some((p) => p.id === selectedId)) {
      setSelectedId(providers[0].id);
    }
  }, [providers, selectedId]);

  const selected = providers.find((p) => p.id === selectedId) ?? null;

  const handleAdd = (kind: ProviderKind) => {
    const id = addProvider(kind);
    setSelectedId(id);
  };

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
      <div className="bg-bg-secondary border border-border rounded-lg w-[720px] h-[480px] shadow-2xl flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-border">
          <h2 className="text-sm font-semibold">LLM Providers</h2>
          <button onClick={onClose} className="text-text-secondary hover:text-text-primary">
            ✕
          </button>
        </div>

        {/* Body: two columns */}
        <div className="flex-1 flex min-h-0">
          {/* Provider list */}
          <div className="w-56 flex-shrink-0 border-r border-border flex flex-col">
            <div className="flex-1 overflow-y-auto py-1">
              {providers.map((p) => (
                <button
                  key={p.id}
                  onClick={() => setSelectedId(p.id)}
                  className={`w-full text-left px-3 py-2 flex items-center gap-2 transition-colors ${
                    selectedId === p.id
                      ? 'bg-accent/10 text-text-primary'
                      : 'text-text-secondary hover:bg-bg-tertiary hover:text-text-primary'
                  }`}
                >
                  <span className={`w-2 h-2 rounded-full flex-shrink-0 ${statusDot(p.status)}`} />
                  <span className="flex flex-col min-w-0">
                    <span className="text-xs truncate">{p.label}</span>
                    <span className="text-[10px] text-text-secondary truncate">
                      {statusLabel(p)}
                    </span>
                  </span>
                </button>
              ))}
              {providers.length === 0 && (
                <p className="px-3 py-4 text-[11px] text-text-secondary italic">
                  No providers yet.
                </p>
              )}
            </div>
            {/* Add provider */}
            <div className="border-t border-border p-2 flex flex-col gap-1">
              <button onClick={() => handleAdd('copilot')} className="w-full flex items-center gap-1.5 px-2 py-1.5 text-xs rounded bg-bg-tertiary hover:bg-border text-text-secondary hover:text-text-primary">
                <Plus size={13} /> GitHub Copilot
              </button>
              <button onClick={() => handleAdd('openai-compatible')} className="w-full flex items-center gap-1.5 px-2 py-1.5 text-xs rounded bg-bg-tertiary hover:bg-border text-text-secondary hover:text-text-primary">
                <Plus size={13} /> OpenAI / Compatible
              </button>
              <button onClick={() => handleAdd('openrouter')} className="w-full flex items-center gap-1.5 px-2 py-1.5 text-xs rounded bg-bg-tertiary hover:bg-border text-text-secondary hover:text-text-primary">
                <Plus size={13} /> OpenRouter
              </button>
              <button onClick={() => handleAdd('deepseek')} className="w-full flex items-center gap-1.5 px-2 py-1.5 text-xs rounded bg-bg-tertiary hover:bg-border text-text-secondary hover:text-text-primary">
                <Plus size={13} /> DeepSeek
              </button>
              <button onClick={() => handleAdd('groq')} className="w-full flex items-center gap-1.5 px-2 py-1.5 text-xs rounded bg-bg-tertiary hover:bg-border text-text-secondary hover:text-text-primary">
                <Plus size={13} /> Groq
              </button>
              <button onClick={() => handleAdd('gemini')} className="w-full flex items-center gap-1.5 px-2 py-1.5 text-xs rounded bg-bg-tertiary hover:bg-border text-text-secondary hover:text-text-primary">
                <Plus size={13} /> Google Gemini
              </button>
              <button onClick={() => handleAdd('ollama')} className="w-full flex items-center gap-1.5 px-2 py-1.5 text-xs rounded bg-bg-tertiary hover:bg-border text-text-secondary hover:text-text-primary">
                <Plus size={13} /> Ollama (local)
              </button>
            </div>
          </div>

          {/* Provider detail */}
          <div className="flex-1 overflow-y-auto p-4">
            {!selected ? (
              <div className="h-full flex items-center justify-center">
                <p className="text-xs text-text-secondary italic">
                  Select or add a provider to configure.
                </p>
              </div>
            ) : selected.kind === 'copilot' ? (
              <CopilotDetail
                provider={selected}
                onUpdate={(patch) => updateProvider(selected.id, patch)}
                onRefresh={() => refreshModels(selected.id)}
                onRemove={() => removeProvider(selected.id)}
              />
            ) : (
              <ApiKeyDetail
                provider={selected}
                onUpdate={(patch) => updateProvider(selected.id, patch)}
                onRefresh={() => refreshModels(selected.id)}
                onRemove={() => removeProvider(selected.id)}
              />
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

interface DetailProps {
  provider: LlmProvider;
  onUpdate: (patch: Partial<LlmProvider>) => void;
  onRefresh: () => Promise<void> | void;
  onRemove: () => void;
}

function RemoveButton({ onRemove }: { onRemove: () => void }) {
  return (
    <button
      onClick={onRemove}
      title="Remove provider"
      className="flex items-center gap-1 px-2 py-1 text-[11px] rounded text-red-400
                 hover:bg-red-500/10 hover:text-red-300 transition-colors"
    >
      <Trash2 size={12} /> Remove
    </button>
  );
}

function ModelChips({ provider }: { provider: LlmProvider }) {
  if (provider.models.length === 0) return null;
  return (
    <div>
      <label className={labelCls}>Discovered models ({provider.models.length})</label>
      <div className="flex flex-wrap gap-1.5 max-h-28 overflow-y-auto">
        {provider.models.map((m) => (
          <span
            key={m.id}
            className="px-2 py-0.5 text-[11px] rounded bg-bg-tertiary border border-border text-text-secondary"
          >
            {m.id}
          </span>
        ))}
      </div>
    </div>
  );
}

/* ── Copilot: device-code flow ─────────────────────────────── */

interface DeviceState {
  verificationUri: string;
  userCode: string;
  deviceCode: string;
  interval: number;
}

function CopilotDetail({ provider, onUpdate, onRefresh, onRemove }: DetailProps) {
  const [device, setDevice] = useState<DeviceState | null>(null);
  const [polling, setPolling] = useState(false);
  const [copied, setCopied] = useState(false);
  const cancelled = useRef(false);

  // Stop polling if the component unmounts (modal closed / provider switched).
  useEffect(() => {
    cancelled.current = false;
    return () => {
      cancelled.current = true;
    };
  }, []);

  const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

  const startAuth = async () => {
    onUpdate({ status: 'authenticating', error: undefined });
    setDevice(null);
    try {
      const d = await api.deviceStart(provider.enterpriseDomain || undefined);
      if (cancelled.current) return;
      setDevice(d);
      setPolling(true);
      // Poll loop driven here so it cancels cleanly on unmount.
      const interval = Math.max(d.interval, 1);
      // eslint-disable-next-line no-constant-condition
      while (!cancelled.current) {
        await sleep((interval + 1) * 1000);
        if (cancelled.current) return;
        const res = await api.devicePoll(d.deviceCode, provider.enterpriseDomain || undefined);
        if (cancelled.current) return;
        if (res.status === 'success' && res.accessToken) {
          onUpdate({ copilotToken: res.accessToken, status: 'authenticated', error: undefined });
          setPolling(false);
          setDevice(null);
          await onRefresh();
          return;
        }
        if (res.status === 'failed') {
          onUpdate({ status: 'error', error: res.error || 'authorization failed' });
          setPolling(false);
          setDevice(null);
          return;
        }
        // pending → keep polling
      }
    } catch (e) {
      if (cancelled.current) return;
      onUpdate({ status: 'error', error: e instanceof Error ? e.message : String(e) });
      setPolling(false);
      setDevice(null);
    }
  };

  const copyCode = async () => {
    if (!device) return;
    try {
      await navigator.clipboard.writeText(device.userCode);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* clipboard unavailable */
    }
  };

  const authed = provider.status === 'authenticated' && !!provider.copilotToken;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <input
          className={`${inputCls} max-w-[60%]`}
          value={provider.label}
          onChange={(e) => onUpdate({ label: e.target.value })}
          placeholder="Label"
        />
        <RemoveButton onRemove={onRemove} />
      </div>

      {/* Enterprise checkbox */}
      <label className="flex items-center gap-2 text-xs text-text-secondary cursor-pointer">
        <input
          type="checkbox"
          className="rounded border-border bg-bg-tertiary accent-accent"
          checked={!!provider.enterpriseDomain}
          onChange={(e) =>
            onUpdate({ enterpriseDomain: e.target.checked ? '' : undefined })
          }
        />
        GitHub Enterprise subscription
      </label>

      {provider.enterpriseDomain !== undefined && (
        <div>
          <label className={labelCls}>Enterprise domain</label>
          <input
            className={inputCls}
            value={provider.enterpriseDomain ?? ''}
            onChange={(e) => onUpdate({ enterpriseDomain: e.target.value })}
            placeholder="github.your-company.com"
          />
        </div>
      )}

      {device ? (
        <div className="rounded border border-border bg-bg-tertiary p-3 space-y-2">
          <p className="text-xs text-text-secondary">
            1. Open{' '}
            <span className="text-accent font-medium break-all">{device.verificationUri}</span>
          </p>
          <p className="text-xs text-text-secondary">2. Enter this code:</p>
          <div className="flex items-center gap-2">
            <code className="text-lg font-bold tracking-widest text-text-primary bg-bg-primary px-3 py-1 rounded border border-border">
              {device.userCode}
            </code>
            <button
              onClick={copyCode}
              title="Copy code"
              className="p-1.5 rounded text-text-secondary hover:text-text-primary hover:bg-border"
            >
              {copied ? <Check size={14} className="text-green-500" /> : <Copy size={14} />}
            </button>
          </div>
          {polling && (
            <div className="flex items-center gap-2 text-xs text-text-secondary pt-1">
              <span className="w-3 h-3 border-2 border-accent/30 border-t-accent rounded-full animate-spin" />
              Waiting for authorization…
            </div>
          )}
        </div>
      ) : (
        <button
          onClick={startAuth}
          disabled={provider.status === 'authenticating'}
          className="px-3 py-1.5 text-xs rounded bg-accent text-white hover:bg-blue-500 transition-colors disabled:opacity-50"
        >
          {authed ? 'Re-authenticate with GitHub' : 'Sign in with GitHub'}
        </button>
      )}

      {provider.status === 'error' && provider.error && (
        <p className="text-xs text-red-400">{provider.error}</p>
      )}

      {authed && (
        <div className="flex items-center gap-2">
          <span className="text-xs text-green-500 flex items-center gap-1">
            <Check size={13} /> Authenticated
          </span>
          <button
            onClick={() => onRefresh()}
            className="flex items-center gap-1 px-2 py-1 text-[11px] rounded bg-bg-tertiary hover:bg-border text-text-secondary hover:text-text-primary"
          >
            <RefreshCw size={11} /> Refresh models
          </button>
        </div>
      )}

      <ModelChips provider={provider} />
    </div>
  );
}

/* ── API-key providers: base URL + API key ──────────────────── */

function ApiKeyDetail({ provider, onUpdate, onRefresh, onRemove }: DetailProps) {
  const [busy, setBusy] = useState(false);
  const [manualId, setManualId] = useState('');

  const save = async () => {
    setBusy(true);
    await onRefresh();
    setBusy(false);
  };

  const addManual = () => {
    const id = manualId.trim();
    if (!id) return;
    if (provider.models.some((m) => m.id === id)) {
      setManualId('');
      return;
    }
    onUpdate({
      models: [...provider.models, { id }],
      status: provider.status === 'error' ? 'key-set' : provider.status,
    });
    setManualId('');
  };

  const fetchFailed = provider.status === 'error';

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <input
          className={`${inputCls} max-w-[60%]`}
          value={provider.label}
          onChange={(e) => onUpdate({ label: e.target.value })}
          placeholder="Label (e.g. OpenAI, DeepSeek)"
        />
        <RemoveButton onRemove={onRemove} />
      </div>

      <div>
        <label className={labelCls}>Base URL</label>
        <input
          className={inputCls}
          value={provider.baseUrl}
          onChange={(e) => onUpdate({ baseUrl: e.target.value })}
          placeholder="https://api.openai.com/v1"
        />
      </div>

      <div>
        <label className={labelCls}>API Key</label>
        <input
          type="password"
          className={inputCls}
          value={provider.apiKey ?? ''}
          onChange={(e) => onUpdate({ apiKey: e.target.value })}
          placeholder="sk-…"
        />
      </div>

      <div className="flex items-center gap-2">
        <button
          onClick={save}
          disabled={busy || !provider.baseUrl.trim()}
          className="px-3 py-1.5 text-xs rounded bg-accent text-white hover:bg-blue-500 transition-colors disabled:opacity-50 flex items-center gap-1.5"
        >
          {busy && (
            <span className="w-3 h-3 border-2 border-white/40 border-t-white rounded-full animate-spin" />
          )}
          Save &amp; fetch models
        </button>
        {provider.status === 'key-set' && !fetchFailed && (
          <span className="text-xs text-green-500 flex items-center gap-1">
            <Check size={13} /> {provider.models.length} models
          </span>
        )}
      </div>

      {fetchFailed && (
        <div className="space-y-2">
          {provider.error && <p className="text-xs text-red-400">{provider.error}</p>}
          <label className={labelCls}>
            Couldn’t auto-discover models — add model IDs manually:
          </label>
          <div className="flex gap-2">
            <input
              className={inputCls}
              value={manualId}
              onChange={(e) => setManualId(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && addManual()}
              placeholder="gpt-4o"
            />
            <button
              onClick={addManual}
              className="px-3 py-1.5 text-xs rounded bg-bg-tertiary hover:bg-border text-text-secondary hover:text-text-primary flex-shrink-0"
            >
              Add
            </button>
          </div>
        </div>
      )}

      <ModelChips provider={provider} />
    </div>
  );
}
