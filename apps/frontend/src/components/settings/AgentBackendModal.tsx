/**
 * AgentBackendModal — three-tab "Agent Backend Settings" dialog.
 *
 * Tabs: Claude Code CLI / Aider · OpenCode / MCP. Persists via agentSettingsStore.
 */
import { useEffect, useState } from 'react';
import {
  useAgentSettingsStore,
  DEFAULT_AGENT_SETTINGS,
} from '../../stores/agentSettingsStore';
import type {
  AgentSettings,
  ClaudeEffort,
} from '../../stores/agentSettingsStore';

interface Props {
  onClose: () => void;
}

type TabId = 'claude' | 'aider' | 'mcp';

const TABS: { id: TabId; label: string }[] = [
  { id: 'claude', label: 'Claude Code CLI' },
  { id: 'aider', label: 'Aider / OpenCode' },
  { id: 'mcp', label: 'MCP' },
];

const inputCls =
  'w-full bg-bg-tertiary text-text-primary text-sm px-2 py-1.5 rounded border border-border ' +
  'focus:outline-none focus:border-accent placeholder:text-text-secondary';
const labelCls = 'text-xs text-text-secondary block mb-1';

export function AgentBackendModal({ onClose }: Props) {
  const stored = useAgentSettingsStore((s) => s.settings);
  const load = useAgentSettingsStore((s) => s.load);
  const save = useAgentSettingsStore((s) => s.save);

  const [tab, setTab] = useState<TabId>('claude');
  const [draft, setDraft] = useState<AgentSettings>(stored ?? DEFAULT_AGENT_SETTINGS);

  useEffect(() => {
    load();
  }, [load]);
  useEffect(() => {
    setDraft(stored);
  }, [stored]);

  const handleSave = async () => {
    await save(draft);
    onClose();
  };

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
      <div className="bg-bg-secondary border border-border rounded-lg w-[560px] shadow-2xl">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-border">
          <h2 className="text-sm font-semibold">Agent Backend Settings</h2>
          <button onClick={onClose} className="text-text-secondary hover:text-text-primary">
            ✕
          </button>
        </div>

        {/* Tabs */}
        <div className="flex items-center gap-1 px-4 pt-3 border-b border-border">
          {TABS.map((t) => (
            <button
              key={t.id}
              onClick={() => setTab(t.id)}
              className={`px-3 py-1.5 text-xs transition-colors border-b-2 -mb-px ${
                tab === t.id
                  ? 'text-text-primary border-accent'
                  : 'text-text-secondary border-transparent hover:text-text-primary'
              }`}
            >
              {t.label}
            </button>
          ))}
        </div>

        {/* Body */}
        <div className="p-4 space-y-3 min-h-[200px]">
          {tab === 'claude' && (
            <>
              <div>
                <label className={labelCls}>Remote Path</label>
                <input
                  className={inputCls}
                  value={draft.claude.remotePath}
                  onChange={(e) =>
                    setDraft((d) => ({ ...d, claude: { ...d.claude, remotePath: e.target.value } }))
                  }
                />
              </div>
              <div>
                <label className={labelCls}>Effort (~effort)</label>
                <div className="flex gap-2">
                  {(['max', 'standard', 'low'] as ClaudeEffort[]).map((e) => (
                    <button
                      key={e}
                      onClick={() => setDraft((d) => ({ ...d, claude: { ...d.claude, effort: e } }))}
                      className={`px-3 py-1 text-xs rounded border transition-colors capitalize ${
                        draft.claude.effort === e
                          ? 'border-accent bg-accent/20 text-text-primary'
                          : 'border-border bg-bg-tertiary text-text-secondary hover:text-text-primary'
                      }`}
                    >
                      {e}
                    </button>
                  ))}
                </div>
              </div>
              <div>
                <label className={labelCls}>ANTHROPIC_BASE_URL</label>
                <input
                  className={inputCls}
                  value={draft.claude.anthropicBaseUrl}
                  onChange={(e) =>
                    setDraft((d) => ({
                      ...d,
                      claude: { ...d.claude, anthropicBaseUrl: e.target.value },
                    }))
                  }
                  placeholder="https://api.anthropic.com (proxy / mirror URL)"
                />
              </div>
            </>
          )}

          {tab === 'aider' && (
            <>
              <div>
                <label className={labelCls}>Remote Path</label>
                <input
                  className={inputCls}
                  value={draft.aider.remotePath}
                  onChange={(e) =>
                    setDraft((d) => ({ ...d, aider: { ...d.aider, remotePath: e.target.value } }))
                  }
                />
              </div>
              <div>
                <label className={labelCls}>Architect Mode (--architect)</label>
                <div className="flex gap-2">
                  {[true, false].map((on) => (
                    <button
                      key={String(on)}
                      onClick={() =>
                        setDraft((d) => ({ ...d, aider: { ...d.aider, architectMode: on } }))
                      }
                      className={`px-3 py-1 text-xs rounded border transition-colors ${
                        draft.aider.architectMode === on
                          ? 'border-accent bg-accent/20 text-text-primary'
                          : 'border-border bg-bg-tertiary text-text-secondary hover:text-text-primary'
                      }`}
                    >
                      {on ? 'On' : 'Off'}
                    </button>
                  ))}
                </div>
              </div>
              <div>
                <label className={labelCls}>Auto Git Commit (--auto-commit)</label>
                <input
                  className={inputCls}
                  value={draft.aider.autoGitCommit}
                  onChange={(e) =>
                    setDraft((d) => ({ ...d, aider: { ...d.aider, autoGitCommit: e.target.value } }))
                  }
                />
              </div>
            </>
          )}

          {tab === 'mcp' && (
            <>
              <div>
                <label className={labelCls}>Remote mcp.json Path</label>
                <input
                  className={inputCls}
                  value={draft.mcp.configPath}
                  onChange={(e) =>
                    setDraft((d) => ({ ...d, mcp: { ...d.mcp, configPath: e.target.value } }))
                  }
                />
              </div>
              <div>
                <label className={labelCls}>Connected MCP Servers</label>
                <div className="flex gap-2">
                  {['Filesystem', 'Postgres', 'Memory'].map((srv) => (
                    <span
                      key={srv}
                      className="px-2.5 py-1 text-xs rounded bg-green-800/60 text-green-200 border border-green-700"
                    >
                      {srv}
                    </span>
                  ))}
                </div>
              </div>
              <div>
                <label className={labelCls}>Status</label>
                <div className={`${inputCls} text-text-secondary`}>
                  3 servers online — last handshake 2s ago
                </div>
              </div>
            </>
          )}
        </div>

        {/* Footer */}
        <div className="flex justify-end gap-2 px-4 py-3 border-t border-border">
          <button
            onClick={onClose}
            className="px-3 py-1.5 text-xs rounded bg-bg-tertiary hover:bg-border text-text-secondary"
          >
            Cancel
          </button>
          <button
            onClick={handleSave}
            className="px-4 py-1.5 text-xs rounded bg-accent text-white hover:bg-blue-500 transition-colors"
          >
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
