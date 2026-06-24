/**
 * ModelListPanel — left-nav tree of LLM providers → models.
 *
 * File-explorer styling: collapsible provider rows, model rows underneath.
 * Clicking a model marks it the active model (✓). The header gear opens the
 * LlmProviderModal for configuration.
 */
import { useEffect, useMemo, useState } from 'react';
import {
  ChevronRight,
  ChevronDown,
  Boxes,
  Check,
  Circle,
  Settings as SettingsIcon,
  AlertCircle,
} from 'lucide-react';
import { useLlmProviderStore } from '../../stores/llmProviderStore';
import { useLayoutStore } from '../../stores/layoutStore';

export function ModelListPanel() {
  const providers = useLlmProviderStore((s) => s.providers);
  const activeModel = useLlmProviderStore((s) => s.activeModel);
  const loaded = useLlmProviderStore((s) => s.loaded);
  const load = useLlmProviderStore((s) => s.load);
  const setActiveModel = useLlmProviderStore((s) => s.setActiveModel);
  const setOpenModal = useLayoutStore((s) => s.setOpenModal);

  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});

  useEffect(() => {
    if (!loaded) load();
  }, [loaded, load]);

  const totalModels = useMemo(
    () => providers.reduce((n, p) => n + p.models.length, 0),
    [providers]
  );

  const toggle = (id: string) =>
    setCollapsed((c) => ({ ...c, [id]: !c[id] }));

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="px-3 py-3 border-b border-border flex items-center justify-between">
        <span className="text-xs font-semibold text-text-secondary uppercase tracking-wider">
          Models
        </span>
        <button
          onClick={() => setOpenModal('llmProviders')}
          title="Configure providers"
          className="text-text-secondary hover:text-text-primary p-0.5"
        >
          <SettingsIcon size={14} />
        </button>
      </div>

      {/* Tree */}
      <div className="flex-1 overflow-y-auto py-1">
        {providers.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full gap-2 px-4 text-center">
            <Boxes size={28} className="text-text-secondary opacity-30" />
            <p className="text-xs text-text-secondary">No providers configured.</p>
            <button
              onClick={() => setOpenModal('llmProviders')}
              className="px-3 py-1.5 text-xs rounded bg-accent text-white hover:bg-blue-500 transition-colors"
            >
              Add a provider
            </button>
          </div>
        ) : (
          providers.map((p) => {
            const isCollapsed = collapsed[p.id];
            const configured =
              p.status === 'authenticated' || p.status === 'key-set';
            return (
              <div key={p.id}>
                {/* Provider row */}
                <button
                  onClick={() => toggle(p.id)}
                  className="w-full flex items-center gap-1 py-1 px-2 hover:bg-bg-tertiary transition-colors text-left"
                  title={p.label}
                >
                  <span className="w-3.5 flex-shrink-0 flex items-center justify-center">
                    {isCollapsed ? (
                      <ChevronRight size={12} className="text-text-secondary" />
                    ) : (
                      <ChevronDown size={12} className="text-text-secondary" />
                    )}
                  </span>
                  <Boxes size={13} className="text-text-secondary flex-shrink-0" />
                  <span
                    className={`text-xs truncate flex-1 ${
                      configured ? 'text-text-primary' : 'text-text-secondary'
                    }`}
                  >
                    {p.label}
                  </span>
                  {p.status === 'error' ? (
                    <AlertCircle size={12} className="text-red-400 flex-shrink-0" />
                  ) : (
                    <span className="text-[10px] text-text-secondary flex-shrink-0">
                      {p.models.length || ''}
                    </span>
                  )}
                </button>

                {/* Model rows */}
                {!isCollapsed &&
                  (p.models.length > 0 ? (
                    p.models.map((m) => {
                      const isActive =
                        activeModel?.providerId === p.id &&
                        activeModel?.modelId === m.id;
                      return (
                        <button
                          key={m.id}
                          onClick={() => setActiveModel(p.id, m.id)}
                          className={`w-full flex items-center gap-1.5 py-0.5 pr-2 hover:bg-bg-tertiary transition-colors text-left ${
                            isActive ? 'bg-accent/10' : ''
                          }`}
                          style={{ paddingLeft: 34 }}
                          title={m.name ? `${m.id} — ${m.name}` : m.id}
                        >
                          {isActive ? (
                            <Check size={12} className="text-accent flex-shrink-0" />
                          ) : (
                            <Circle
                              size={6}
                              className="text-text-secondary/40 flex-shrink-0 mx-[3px]"
                            />
                          )}
                          <span
                            className={`text-xs truncate ${
                              isActive ? 'text-accent' : 'text-text-primary'
                            }`}
                          >
                            {m.id}
                          </span>
                        </button>
                      );
                    })
                  ) : (
                    <button
                      onClick={() => setOpenModal('llmProviders')}
                      className="w-full text-left py-0.5 text-[11px] text-text-secondary italic hover:text-text-primary"
                      style={{ paddingLeft: 34 }}
                    >
                      {configured ? 'No models — refresh in settings' : 'Configure…'}
                    </button>
                  ))}
              </div>
            );
          })
        )}
      </div>

      {/* Footer summary */}
      {providers.length > 0 && (
        <div className="px-3 py-1.5 border-t border-border text-[10px] text-text-secondary">
          {providers.length} provider{providers.length === 1 ? '' : 's'} · {totalModels} model
          {totalModels === 1 ? '' : 's'}
        </div>
      )}
    </div>
  );
}
