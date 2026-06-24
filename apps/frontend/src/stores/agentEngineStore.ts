/**
 * Agent Engine Store — persisted per-agent launch configuration for the
 * "Agent Engine Settings" modal. Persistence is localStorage (same lightweight
 * pattern as layoutStore) since this is purely frontend launch config.
 */
import { create } from 'zustand';

const STORE_KEY = 'remote-ai-ide:agent-engine';

export type AgentKind = 'claude' | 'opencode' | 'codex' | 'hermes';

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
}

export interface LastConn {
  host: string;
  port: number;
  user: string;
  authMethod: 'key' | 'password' | 'agent';
}

function emptyConfig(): AgentEngineConfig {
  return { workDir: '', argPresets: [], extraArgs: '', envModels: {}, extraEnv: [] };
}

const DEFAULT_CONFIGS: Record<AgentKind, AgentEngineConfig> = {
  claude: emptyConfig(),
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

function loadPersisted(): Persisted {
  try {
    const raw = localStorage.getItem(STORE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<Persisted>;
      return {
        configs: { ...DEFAULT_CONFIGS, ...(parsed.configs ?? {}) },
        lastConn: { ...DEFAULT_CONN, ...(parsed.lastConn ?? {}) },
        profiles: parsed.profiles ?? [],
      };
    }
  } catch {
    /* ignore */
  }
  return { configs: DEFAULT_CONFIGS, lastConn: DEFAULT_CONN, profiles: [] };
}

function persist(state: Persisted) {
  try {
    localStorage.setItem(STORE_KEY, JSON.stringify(state));
  } catch {
    /* ignore */
  }
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
  configs: Record<AgentKind, AgentEngineConfig>;
  lastConn: LastConn;
  profiles: AgentProfile[];
  setConfig: (kind: AgentKind, patch: Partial<AgentEngineConfig>) => void;
  setLastConn: (patch: Partial<LastConn>) => void;
  addProfile: (p: Omit<AgentProfile, 'id'>) => string;
  removeProfile: (id: string) => void;
}

export const useAgentEngineStore = create<AgentEngineStore>((set, get) => {
  const initial = loadPersisted();
  return {
    configs: initial.configs,
    lastConn: initial.lastConn,
    profiles: initial.profiles ?? [],

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
  };
});
