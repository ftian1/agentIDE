/**
 * Agent Settings Store — persisted "Agent Backend Settings" (Claude / Aider / MCP).
 *
 * Persistence is backed by SQLite via the load/save_agent_settings Tauri
 * commands. Mirrors the design's three-tab modal.
 */
import { create } from 'zustand';
import { createSettingsApi } from '../api/settingsApi';

export type ClaudeEffort = 'max' | 'standard' | 'low';

export interface ClaudeSettings {
  remotePath: string;
  effort: ClaudeEffort;
  anthropicBaseUrl: string;
}

export interface AiderSettings {
  remotePath: string;
  architectMode: boolean;
  autoGitCommit: string;
}

export interface McpSettings {
  configPath: string;
}

export interface AgentSettings {
  claude: ClaudeSettings;
  aider: AiderSettings;
  mcp: McpSettings;
}

export const DEFAULT_AGENT_SETTINGS: AgentSettings = {
  claude: {
    remotePath: '/usr/local/bin/claude',
    effort: 'max',
    anthropicBaseUrl: 'https://api.anthropic.com',
  },
  aider: {
    remotePath: '/usr/local/bin/aider',
    architectMode: true,
    autoGitCommit: 'Enabled — commit after each successful edit',
  },
  mcp: {
    configPath: '~/.config/agent/mcp.json',
  },
};

const api = createSettingsApi();

interface AgentSettingsStore {
  settings: AgentSettings;
  loaded: boolean;
  load: () => Promise<void>;
  save: (next: AgentSettings) => Promise<void>;
}

export const useAgentSettingsStore = create<AgentSettingsStore>((set) => ({
  settings: DEFAULT_AGENT_SETTINGS,
  loaded: false,

  async load() {
    try {
      const loaded = await api.load();
      if (loaded) {
        set({ settings: { ...DEFAULT_AGENT_SETTINGS, ...loaded }, loaded: true });
        return;
      }
    } catch {
      // Tauri unavailable or no saved settings — keep defaults.
    }
    set({ loaded: true });
  },

  async save(next) {
    set({ settings: next });
    try {
      await api.save(next);
    } catch {
      // TODO: surface save failure (command may be unavailable in dev).
    }
  },
}));
