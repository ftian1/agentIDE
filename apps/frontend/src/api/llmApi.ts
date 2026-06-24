/**
 * LLM API — Tauri command wrappers for multi-provider LLM configuration.
 *
 * Covers GitHub Copilot device-code auth, model discovery, and persistence
 * of providers + the active-model selection.
 */
import { invoke } from '@tauri-apps/api/core';
import type {
  LlmProvider,
  ModelInfo,
  ActiveModel,
} from '../stores/llmProviderStore';

export interface DeviceCodeResp {
  verificationUri: string;
  userCode: string;
  deviceCode: string;
  interval: number;
}

export interface PollResp {
  status: 'pending' | 'success' | 'failed';
  accessToken?: string;
  error?: string;
}

/** Subset of a provider sent to the backend for model discovery. */
export interface ProviderFetchInput {
  kind: LlmProvider['kind'];
  baseUrl?: string;
  apiKey?: string;
  copilotToken?: string;
  enterpriseDomain?: string;
}

export interface LlmApi {
  deviceStart: (enterpriseDomain?: string) => Promise<DeviceCodeResp>;
  devicePoll: (deviceCode: string, enterpriseDomain?: string) => Promise<PollResp>;
  fetchModels: (provider: ProviderFetchInput) => Promise<ModelInfo[]>;
  loadProviders: () => Promise<LlmProvider[] | null>;
  saveProviders: (providers: LlmProvider[]) => Promise<void>;
  loadActiveModel: () => Promise<ActiveModel | null>;
  saveActiveModel: (active: ActiveModel) => Promise<void>;
}

export function createLlmApi(): LlmApi {
  return {
    deviceStart: (enterpriseDomain) =>
      invoke<DeviceCodeResp>('copilot_device_start', { enterpriseDomain }),
    devicePoll: (deviceCode, enterpriseDomain) =>
      invoke<PollResp>('copilot_device_poll', { deviceCode, enterpriseDomain }),
    fetchModels: (provider) => invoke<ModelInfo[]>('llm_fetch_models', { provider }),
    loadProviders: () => invoke<LlmProvider[] | null>('load_llm_providers'),
    saveProviders: (providers) => invoke<void>('save_llm_providers', { providers }),
    loadActiveModel: () => invoke<ActiveModel | null>('load_active_model'),
    saveActiveModel: (active) => invoke<void>('save_active_model', { active }),
  };
}
