/**
 * Shared spawn-env builder — used by both AgentEngineModal and AgentManagerPanel
 * so that model env vars, auth key, and provider routing are always included
 * regardless of which UI path launches the session.
 */
import type { AgentKind, AgentEngineConfig } from '../stores/agentEngineStore';
import type { LlmProvider, ProviderKind } from '../stores/llmProviderStore';

/**
 * Auth environment variable injected for each agent CLI in passthrough mode
 * (user provides a direct API key via agent settings).
 */
export const AGENT_AUTH_ENV: Record<AgentKind, string> = {
  claude: 'ANTHROPIC_API_KEY',
  copilot: 'COPILOT_GITHUB_TOKEN',
  gemini: 'GEMINI_API_KEY',
  opencode: 'OPENCODE_API_KEY',
  codex: 'OPENAI_API_KEY',
  hermes: 'OPENROUTER_API_KEY',
};

/**
 * Model-selection env var for each agent CLI.  When the user has selected a
 * model from a configured LLM provider, this env var tells the CLI which model
 * ID to send in its API requests — which the proxy then matches against
 * provider `modelIds` lists to pick the correct upstream + auth.
 *
 * Claude is excluded because it uses `AgentEngineConfig.envModels` (a separate
 * UI with per-model-env dropdowns: ANTHROPIC_MODEL, ANTHROPIC_DEFAULT_OPUS_MODEL,
 * etc.).
 */
const AGENT_MODEL_ENV: Partial<Record<AgentKind, string>> = {
  copilot: 'COPILOT_MODEL',
  gemini: 'GEMINI_MODEL',
  opencode: 'OPENAI_MODEL',
  codex: 'OPENAI_MODEL',
  hermes: 'HERMES_MODEL',
};

/**
 * Anthropic-compatible API endpoint path prefix for non-Anthropic providers.
 * Mirrors `provider_path_prefix` in crates/remote-agent-host/src/tap/mod.rs.
 */
const ANTHROPIC_PATH_PREFIX: Partial<Record<ProviderKind, string>> = {
  deepseek: '/anthropic',
};

/** Pick a model ID to tell the CLI to use when in provider-routing mode. */
function resolveModel(
  agent: AgentKind,
  providers: LlmProvider[],
  activeModelId?: string,
): string | null {
  // Claude already has envModels; don't override.
  if (agent === 'claude') return null;

  // Use the model the user currently has selected in the UI.
  if (activeModelId) return activeModelId;

  // Fallback: first model of the first provider that has models.
  for (const p of providers) {
    const m = p.models[0];
    if (m) return m.id;
  }
  return null;
}

/** Construct the Anthropic-compatible base URL for a provider. */
function anthropicBaseUrl(p: LlmProvider): string {
  const prefix = ANTHROPIC_PATH_PREFIX[p.kind];
  if (!prefix) return p.baseUrl;
  const m = p.baseUrl.match(/^(https?:\/\/[^/]+)/);
  return `${m ? m[1] : p.baseUrl}${prefix}`;
}

/** Find the provider that owns the given model ID. */
function findProviderForModel(modelId: string, providers: LlmProvider[]): LlmProvider | undefined {
  return providers.find((p) => p.models.some((m) => m.id === modelId));
}

/**
 * Build the full environment map to send with a SpawnSession request.
 *
 * Two auth modes:
 *
 *  1. **Passthrough** (authKey set):
 *     The agent-specific auth env var (ANTHROPIC_API_KEY / GEMINI_API_KEY /
 *     …) is set to the user's real key.  The CLI connects to its default
 *     upstream through the MITM proxy; the proxy records but does not
 *     modify auth or routing.
 *
 *  2. **Provider routing** (no authKey, LLM providers configured):
 *     Providers are serialised into `__providers_json` so the remote
 *     proxy can route by model.  For Claude Code CLI, ANTHROPIC_BASE_URL
 *     points the CLI at the provider's Anthropic-compatible endpoint
 *     and ANTHROPIC_AUTH_TOKEN carries the provider's API key — the CLI
 *     talks directly to the correct upstream through the MITM proxy,
 *     which intercepts and can reroute if a different model matches.
 */
export function buildSpawnEnv(
  agent: AgentKind,
  cfg: AgentEngineConfig,
  providers: LlmProvider[],
  activeModelId?: string,
): Record<string, string> {
  const env: Record<string, string> = {};

  // ── Model env vars (Claude-specific) ──────────────────────────────
  if (agent === 'claude') {
    for (const [k, v] of Object.entries(cfg.envModels)) {
      if (v) env[k] = v;
    }
  }

  // ── Generic extra env ─────────────────────────────────────────────
  for (const { key, value } of cfg.extraEnv) {
    if (key.trim()) env[key.trim()] = value;
  }

  // ── Auth & Provider Routing ───────────────────────────────────────
  const cliAuthKey = cfg.authKey?.trim();
  if (cliAuthKey) {
    // ── Mode 1: Passthrough ─────────────────────────────────────────
    env[AGENT_AUTH_ENV[agent]] = cliAuthKey;
  } else {
    // ── Mode 2: Provider routing (all agents via unified proxy) ─────
    const credProviders = providers.filter((p) => p.apiKey || p.copilotToken);
    const providerConfigs = credProviders.map((p) => ({
      id: p.id,
      kind: p.kind,
      label: p.label,
      baseUrl: p.baseUrl,
      apiKey: p.apiKey,
      copilotToken: p.copilotToken,
      modelIds: p.models.map((m) => m.id),
    }));
    if (providerConfigs.length > 0) {
      env.__providers_json = JSON.stringify(providerConfigs);

      const model = resolveModel(agent, providers, activeModelId);
      const modelEnv = AGENT_MODEL_ENV[agent];
      if (model && modelEnv) {
        env[modelEnv] = model;
      }
    }

    // ── Claude Code CLI: point ANTHROPIC_BASE_URL at the provider ──
    // In Mitm mode the CLI connects through HTTP_PROXY; setting
    // ANTHROPIC_BASE_URL tells it to target the third-party endpoint
    // directly instead of api.anthropic.com.  The proxy intercepts
    // all traffic regardless of target host so model-based rerouting
    // still works when a request's model field doesn't match.
    if (agent === 'claude') {
      const apiKeyProviders = providers.filter((p) => p.apiKey);
      // Pick the provider whose model matches cfg.envModels, or
      // activeModelId, or the first with an API key.
      let pick: LlmProvider | undefined;
      for (const modelId of Object.values(cfg.envModels)) {
        if (!modelId) continue;
        pick = findProviderForModel(modelId, apiKeyProviders);
        if (pick) break;
      }
      if (!pick && activeModelId) {
        pick = findProviderForModel(activeModelId, apiKeyProviders);
      }
      if (!pick && apiKeyProviders.length > 0) {
        pick = apiKeyProviders[0];
      }
      if (pick) {
        env.ANTHROPIC_BASE_URL = anthropicBaseUrl(pick);
        env.ANTHROPIC_AUTH_TOKEN = pick.apiKey!;
      }
    }

  }

  return env;
}
