/**
 * Settings API — Tauri command wrappers for agent backend settings.
 */
import { invoke } from '@tauri-apps/api/core';
import type { AgentSettings } from '../stores/agentSettingsStore';

export interface SettingsApi {
  load: () => Promise<AgentSettings | null>;
  save: (settings: AgentSettings) => Promise<void>;
}

export function createSettingsApi(): SettingsApi {
  return {
    load: () => invoke<AgentSettings | null>('load_agent_settings'),
    save: (settings) => invoke<void>('save_agent_settings', { settings }),
  };
}
