/**
 * HttpTrafficPanel — bottom-panel tab showing captured agent CLI HTTP traffic.
 *
 * Header: tap on/off + mode (MITM/Reverse) + clear. Body: a list of exchanges
 * (method · host · path · status · duration); click a row to expand request and
 * response headers + decoded bodies.
 */
import { useEffect, useMemo, useState } from 'react';
import { RefreshCw, Trash2, Lock, Globe } from 'lucide-react';
import { useHttpTrafficStore, type TrafficRecord } from '../../stores/httpTrafficStore';
import { useConnectionStore } from '../../stores/connectionStore';

const td = new TextDecoder('utf-8', { fatal: false });

function decodeBody(bytes: number[]): string {
  if (!bytes || bytes.length === 0) return '';
  try {
    return td.decode(new Uint8Array(bytes));
  } catch {
    return `<${bytes.length} bytes>`;
  }
}

function pathOf(url: string): string {
  try {
    return new URL(url).pathname || '/';
  } catch {
    return url;
  }
}

function statusColor(status: number): string {
  if (status >= 500) return 'text-red-400';
  if (status >= 400) return 'text-yellow-400';
  if (status >= 200 && status < 300) return 'text-green-400';
  return 'text-text-secondary';
}

function HeaderTable({ headers }: { headers: Record<string, string> }) {
  const entries = Object.entries(headers);
  if (entries.length === 0) return <p className="text-text-secondary italic">（无）</p>;
  return (
    <div className="space-y-0.5">
      {entries.map(([k, v]) => (
        <div key={k} className="flex gap-2">
          <span className="text-text-secondary flex-shrink-0">{k}:</span>
          <span className="text-text-primary break-all">{v}</span>
        </div>
      ))}
    </div>
  );
}

function ExchangeDetail({ rec }: { rec: TrafficRecord }) {
  const ex = rec.exchange;
  const reqBody = useMemo(() => decodeBody(ex.reqBody), [ex.reqBody]);
  const respBody = useMemo(() => decodeBody(ex.respBody), [ex.respBody]);

  return (
    <div className="px-6 py-2 bg-bg-primary border-y border-border font-mono text-[11px] space-y-3">
      <div>
        <div className="text-text-secondary uppercase tracking-wider mb-1">Request</div>
        <HeaderTable headers={ex.reqHeaders} />
        {reqBody && (
          <pre className="mt-1 whitespace-pre-wrap break-all text-text-primary max-h-48 overflow-y-auto">
            {reqBody}
          </pre>
        )}
      </div>
      <div>
        <div className="text-text-secondary uppercase tracking-wider mb-1">Response</div>
        <HeaderTable headers={ex.respHeaders} />
        {respBody && (
          <pre className="mt-1 whitespace-pre-wrap break-all text-text-primary max-h-64 overflow-y-auto">
            {respBody}
          </pre>
        )}
        {ex.truncated && (
          <p className="text-yellow-400 mt-1">⚠ body truncated at capture cap</p>
        )}
      </div>
    </div>
  );
}

export function HttpTrafficPanel() {
  const exchanges = useHttpTrafficStore((s) => s.exchanges);
  const settings = useHttpTrafficStore((s) => s.settings);
  const loaded = useHttpTrafficStore((s) => s.loaded);
  const loadSettings = useHttpTrafficStore((s) => s.loadSettings);
  const setSettings = useHttpTrafficStore((s) => s.setSettings);
  const loadTraces = useHttpTrafficStore((s) => s.loadTraces);
  const clear = useHttpTrafficStore((s) => s.clear);
  const activeConnectionId = useConnectionStore((s) => s.activeConnectionId);

  const [expanded, setExpanded] = useState<string | null>(null);

  useEffect(() => {
    if (!loaded) loadSettings();
  }, [loaded, loadSettings]);

  // Reverse order: newest first.
  const rows = useMemo(() => [...exchanges].reverse(), [exchanges]);

  const toggleEnabled = () =>
    setSettings({ ...settings, enabled: !settings.enabled });
  const toggleMode = () =>
    setSettings({ ...settings, mode: settings.mode === 'mitm' ? 'reverse' : 'mitm' });

  return (
    <div className="flex flex-col h-full">
      {/* Header / controls */}
      <div className="flex items-center gap-2 px-3 py-1.5 border-b border-border text-xs flex-shrink-0">
        <button
          onClick={toggleEnabled}
          className={`px-2 py-0.5 rounded border transition-colors ${
            settings.enabled
              ? 'border-green-600 bg-green-800/40 text-green-200'
              : 'border-border bg-bg-tertiary text-text-secondary hover:text-text-primary'
          }`}
          title="Toggle HTTP traffic capture for new sessions"
        >
          {settings.enabled ? '● Tap On' : '○ Tap Off'}
        </button>
        <button
          onClick={toggleMode}
          className="px-2 py-0.5 rounded border border-border bg-bg-tertiary text-text-secondary hover:text-text-primary flex items-center gap-1"
          title="Capture mode"
        >
          {settings.mode === 'mitm' ? <Lock size={11} /> : <Globe size={11} />}
          {settings.mode === 'mitm' ? 'MITM (all HTTPS)' : 'Reverse (LLM API)'}
        </button>
        <div className="flex-1" />
        <span className="text-text-secondary">{exchanges.length} exchanges</span>
        <button
          onClick={() => activeConnectionId && loadTraces(activeConnectionId)}
          disabled={!activeConnectionId}
          className="p-1 rounded text-text-secondary hover:text-text-primary hover:bg-bg-tertiary disabled:opacity-40"
          title="Reload persisted traces"
        >
          <RefreshCw size={12} />
        </button>
        <button
          onClick={() => clear()}
          className="p-1 rounded text-text-secondary hover:text-red-300 hover:bg-bg-tertiary"
          title="Clear view"
        >
          <Trash2 size={12} />
        </button>
      </div>

      {/* Notice when off */}
      {!settings.enabled && exchanges.length === 0 && (
        <div className="px-3 py-1 text-[11px] text-text-secondary bg-bg-tertiary/40 border-b border-border">
          Tap is off — enable it, then start a new agent session to capture traffic.
        </div>
      )}

      {/* List */}
      <div className="flex-1 overflow-y-auto font-mono text-xs">
        {rows.length === 0 ? (
          <div className="flex items-center justify-center h-full">
            <p className="text-text-secondary italic">暂无 HTTP 流量</p>
          </div>
        ) : (
          rows.map((rec) => {
            const ex = rec.exchange;
            const isOpen = expanded === ex.exchangeId;
            return (
              <div key={ex.exchangeId}>
                <button
                  onClick={() => setExpanded(isOpen ? null : ex.exchangeId)}
                  className={`w-full flex items-center gap-2 px-3 py-1 text-left hover:bg-bg-tertiary transition-colors ${
                    isOpen ? 'bg-bg-tertiary' : ''
                  }`}
                >
                  <span className="text-accent w-12 flex-shrink-0">{ex.method}</span>
                  <span className={`w-10 flex-shrink-0 ${statusColor(ex.status)}`}>
                    {ex.status || '—'}
                  </span>
                  <span className="text-text-secondary flex-shrink-0">{ex.host}</span>
                  <span className="text-text-primary truncate flex-1">{pathOf(ex.url)}</span>
                  <span className="text-text-secondary flex-shrink-0">{ex.durationMs}ms</span>
                </button>
                {isOpen && <ExchangeDetail rec={rec} />}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
