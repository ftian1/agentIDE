/**
 * LLM Provider Store — persisted multi-provider LLM configuration.
 *
 * Holds the list of configured providers (GitHub Copilot via OAuth device-code,
 * or any OpenAI-compatible endpoint) plus the active-model selection. Persisted
 * through the load/save_llm_providers + load/save_active_model Tauri commands.
 *
 * Selector discipline: components must select stable slices (e.g. `s.providers`)
 * and derive arrays with useMemo — returning fresh arrays from a selector has
 * triggered React #185 infinite re-renders in this project before.
 */
import { create } from 'zustand';
import { createLlmApi } from '../api/llmApi';
import type { ProviderFetchInput } from '../api/llmApi';

export type ProviderKind =
  | 'copilot'
  | 'openai-compatible'
  | 'ollama'
  | 'openrouter'
  | 'deepseek'
  | 'groq'
  | 'gemini';

export type ProviderStatus =
  | 'unconfigured'
  | 'authenticating'
  | 'authenticated'
  | 'key-set'
  | 'error';

export interface ModelInfo {
  id: string;
  name?: string;
}

export interface LlmProvider {
  id: string;
  kind: ProviderKind;
  label: string;
  baseUrl: string;
  apiKey?: string;
  copilotToken?: string;
  enterpriseDomain?: string;
  models: ModelInfo[];
  status: ProviderStatus;
  error?: string;
}

export interface ActiveModel {
  providerId: string;
  modelId: string;
}

const api = createLlmApi();

function uid(): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
    return crypto.randomUUID();
  }
  return `p_${Date.now()}_${Math.floor(Math.random() * 1e6)}`;
}

function toFetchInput(p: LlmProvider): ProviderFetchInput {
  return {
    kind: p.kind,
    baseUrl: p.baseUrl,
    apiKey: p.apiKey,
    copilotToken: p.copilotToken,
    enterpriseDomain: p.enterpriseDomain,
  };
}

const PROVIDER_DEFAULTS: Record<ProviderKind, { label: string; baseUrl: string }> = {
  copilot:           { label: 'GitHub Copilot',     baseUrl: '' },
  'openai-compatible': { label: 'OpenAI',             baseUrl: 'https://api.openai.com/v1' },
  ollama:            { label: 'Ollama (local)',       baseUrl: 'http://localhost:11434/v1' },
  openrouter:        { label: 'OpenRouter',           baseUrl: 'https://openrouter.ai/api/v1' },
  deepseek:          { label: 'DeepSeek',             baseUrl: 'https://api.deepseek.com/v1' },
  groq:              { label: 'Groq',                 baseUrl: 'https://api.groq.com/openai/v1' },
  gemini:            { label: 'Google Gemini',        baseUrl: 'https://generativelanguage.googleapis.com/v1beta/openai' },
};

export function newProvider(kind: ProviderKind): LlmProvider {
  const def = PROVIDER_DEFAULTS[kind];
  return {
    id: uid(),
    kind,
    label: def.label,
    baseUrl: def.baseUrl,
    models: [],
    status: 'unconfigured',
  };
}

interface LlmProviderStore {
  providers: LlmProvider[];
  activeModel: ActiveModel | null;
  loaded: boolean;

  load: () => Promise<void>;
  addProvider: (kind: ProviderKind) => string;
  updateProvider: (id: string, patch: Partial<LlmProvider>) => void;
  removeProvider: (id: string) => void;
  setActiveModel: (providerId: string, modelId: string) => void;
  refreshModels: (id: string) => Promise<void>;
}

/** Persist providers to the DB (fire-and-forget; dev may lack the command). */
function persistProviders(providers: LlmProvider[]) {
  api.saveProviders(providers).catch(() => {
    /* command unavailable in dev — ignore */
  });
}

export const useLlmProviderStore = create<LlmProviderStore>((set, get) => ({
  providers: [],
  activeModel: null,
  loaded: false,

  async load() {
    try {
      const [providers, active] = await Promise.all([
        api.loadProviders(),
        api.loadActiveModel(),
      ]);
      set({
        providers: providers ?? [],
        activeModel: active ?? null,
        loaded: true,
      });
      return;
    } catch {
      // Tauri unavailable — start empty.
    }
    set({ loaded: true });
  },

  addProvider(kind) {
    const p = newProvider(kind);
    set((s) => {
      const providers = [...s.providers, p];
      persistProviders(providers);
      return { providers };
    });
    return p.id;
  },

  updateProvider(id, patch) {
    set((s) => {
      const providers = s.providers.map((p) =>
        p.id === id ? { ...p, ...patch } : p
      );
      persistProviders(providers);
      return { providers };
    });
  },

  removeProvider(id) {
    set((s) => {
      const providers = s.providers.filter((p) => p.id !== id);
      persistProviders(providers);
      // Clear active model if it pointed at this provider.
      const activeModel =
        s.activeModel?.providerId === id ? null : s.activeModel;
      if (activeModel !== s.activeModel && activeModel === null) {
        api.saveActiveModel({ providerId: '', modelId: '' }).catch(() => {});
      }
      return { providers, activeModel };
    });
  },

  setActiveModel(providerId, modelId) {
    const active: ActiveModel = { providerId, modelId };
    set({ activeModel: active });
    api.saveActiveModel(active).catch(() => {});
  },

  async refreshModels(id) {
    const provider = get().providers.find((p) => p.id === id);
    if (!provider) return;
    try {
      const models = await api.fetchModels(toFetchInput(provider));
      get().updateProvider(id, {
        models,
        error: undefined,
        status:
          provider.kind === 'copilot' ? 'authenticated' : 'key-set',
      });
    } catch (e) {
      get().updateProvider(id, {
        status: 'error',
        error: e instanceof Error ? e.message : String(e),
      });
    }
  },
}));
