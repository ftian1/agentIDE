/**
 * Shared spawn-env builder — used by both AgentEngineModal and AgentManagerPanel
 * so that model env vars, auth key, and provider routing are always included
 * regardless of which UI path launches the session.
 */
import type { AgentKind, AgentEngineConfig } from '../stores/agentEngineStore';
import type { LlmProvider } from '../stores/llmProviderStore';

/**
 * Auth environment variable injected for each agent CLI when the user provides
 * a direct API key via agent settings.  The key value is taken from
 * `AgentEngineConfig.authKey` and set as this env var on the remote CLI process.
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

/**
 * Build the full environment map to send with a SpawnSession request.
 *
 * Two auth modes (both available for every agent type):
 *
 *  1. **Passthrough** (CLI auth key set):
 *     The agent-specific auth env var (ANTHROPIC_API_KEY /
 *     GEMINI_API_KEY / …) is set to the user's real key.  The remote
 *     proxy captures but does NOT modify auth or routing — the CLI
 *     talks directly to its default upstream with the user's key.
 *
 *  2. **Provider routing** (no auth key, LLM providers configured):
 *     All providers with credentials are serialised into
 *     `__providers_json`.  On the remote host this triggers the
 *     unified HTTP proxy which:
 *       - starts in Reverse mode
 *       - injects every known base-URL env var
 *         (ANTHROPIC_BASE_URL / OPENAI_BASE_URL /
 *          COPILOT_PROVIDER_BASE_URL / …) pointing to itself
 *       - sets dummy auth tokens so no CLI blocks on /login
 *       - parses `model` from each request body and routes to the
 *         matching provider's upstream with its real credentials
 *
 *     Additionally, the active model ID is passed to the CLI via the
 *     agent-specific model env var (e.g. COPILOT_MODEL, OPENAI_MODEL)
 *     so the CLI sends requests for a model the proxy can match.
 *
 *     The proxy approach works uniformly across Claude Code, GitHub
 *     Copilot CLI, OpenCode, Codex, Hermes, and Gemini CLI — each
 *     finds its own base-URL env var redirected to the proxy and
 *     the proxy handles the rest.
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
    const providerConfigs = providers
      .filter((p) => p.apiKey || p.copilotToken)
      .map((p) => ({
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

      // Tell the CLI which model to use — the proxy matches this
      // model field against provider modelIds to route the request.
      const model = resolveModel(agent, providers, activeModelId);
      const modelEnv = AGENT_MODEL_ENV[agent];
      if (model && modelEnv) {
        env[modelEnv] = model;
      }
    }
  }

  return env;
}
