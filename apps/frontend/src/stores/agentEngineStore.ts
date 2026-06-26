/**
 * Agent Engine Store — persisted per-agent launch configuration.
 * Persisted to SQLite (primary) + localStorage (fallback).
 */
import { create } from 'zustand';
import { loadPersisted, savePersisted } from '../lib/storage';

const STORE_KEY = 'remote-ai-ide:agent-engine';

export type AgentKind = 'claude' | 'copilot' | 'gemini' | 'opencode' | 'codex' | 'hermes';

export interface AgentEngineConfig {
  workDir: string;
  /** Selected launch-arg preset ids (Claude). Expanded to tokens at launch. */
  argPresets: string[];
  /** Free-text extra args, space-split at launch. */
  extraArgs: string;
  /** Claude env→model map, e.g. ANTHROPIC_MODEL → modelId. */
  envModels: Record<string, string>;
  /** Generic key/value env vars (non-Claude agents, or extras). */
  extraEnv: { key: string; value: string }[];
  /** Direct API key for the CLI. When set, passes the agent-specific auth
   *  env var (ANTHROPIC_API_KEY / GEMINI_API_KEY / COPILOT_GITHUB_TOKEN /
   *  OPENCODE_API_KEY / OPENROUTER_API_KEY / OPENAI_API_KEY).
   *  When empty, the gateway/proxy handles auth. */
  authKey: string;
}

export interface LastConn {
  host: string;
  port: number;
  user: string;
  authMethod: 'key' | 'password' | 'agent';
}

function emptyConfig(): AgentEngineConfig {
  return { workDir: '', argPresets: [], extraArgs: '', envModels: {}, extraEnv: [], authKey: '' };
}

const DEFAULT_CONFIGS: Record<AgentKind, AgentEngineConfig> = {
  claude: emptyConfig(),
  copilot: emptyConfig(),
  gemini: emptyConfig(),
  opencode: emptyConfig(),
  codex: emptyConfig(),
  hermes: emptyConfig(),
};

const DEFAULT_CONN: LastConn = { host: '', port: 22, user: '', authMethod: 'password' };

interface Persisted {
  configs: Record<AgentKind, AgentEngineConfig>;
  lastConn: LastConn;
  profiles: AgentProfile[];
}

function persist(state: Persisted) {
  savePersisted(STORE_KEY, state);
}

export interface AgentProfile {
  id: string;
  name: string;
  kind: AgentKind;
  connectionId?: string;
  lastConn: LastConn;
  config: AgentEngineConfig;
}

export const AGENT_LABELS: Record<AgentKind, string> = {
  claude: 'Claude Code',
  copilot: 'GitHub Copilot',
  gemini: 'Gemini CLI',
  opencode: 'OpenCode',
  codex: 'Codex',
  hermes: 'Hermes',
};

function uid(): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
    return crypto.randomUUID();
  }
  return `ap_${Date.now()}_${Math.floor(Math.random() * 1e6)}`;
}

interface AgentEngineStore {
  _init: () => Promise<void>;
  configs: Record<AgentKind, AgentEngineConfig>;
  lastConn: LastConn;
  profiles: AgentProfile[];
  setConfig: (kind: AgentKind, patch: Partial<AgentEngineConfig>) => void;
  setLastConn: (patch: Partial<LastConn>) => void;
  addProfile: (p: Omit<AgentProfile, 'id'>) => string;
  removeProfile: (id: string) => void;
}

export const useAgentEngineStore = create<AgentEngineStore>((set, get) => ({
    _init: async () => {
      const saved = await loadPersisted<Persisted>(STORE_KEY, { configs: DEFAULT_CONFIGS, lastConn: DEFAULT_CONN, profiles: [] as AgentProfile[] });
      set({
        configs: { ...DEFAULT_CONFIGS, ...(saved.configs ?? {}) },
        lastConn: { ...DEFAULT_CONN, ...(saved.lastConn ?? {}) },
        profiles: saved.profiles ?? [],
      });
    },

    configs: DEFAULT_CONFIGS,
    lastConn: DEFAULT_CONN,
    profiles: [],

    setConfig(kind, patch) {
      set((s) => {
        const configs = { ...s.configs, [kind]: { ...s.configs[kind], ...patch } };
        persist({ configs, lastConn: s.lastConn, profiles: s.profiles });
        return { configs };
      });
    },

    setLastConn(patch) {
      set((s) => {
        const lastConn = { ...s.lastConn, ...patch };
        persist({ configs: s.configs, lastConn, profiles: s.profiles });
        return { lastConn };
      });
    },

    addProfile(p) {
      const profile: AgentProfile = { ...p, id: uid() };
      set((s) => {
        const profiles = [...s.profiles, profile];
        persist({ configs: s.configs, lastConn: s.lastConn, profiles });
        return { profiles };
      });
      return profile.id;
    },

    removeProfile(id) {
      set((s) => {
        const profiles = s.profiles.filter(p => p.id !== id);
        persist({ configs: s.configs, lastConn: s.lastConn, profiles });
        return { profiles };
      });
    },
}));
