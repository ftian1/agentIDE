/**
 * Usage Parser — extracts token usage and model ID from raw HTTP
 * request/response bodies (Anthropic & OpenAI APIs).
 *
 * Anthropic Messages API:
 *   - Request body:  { "model": "claude-sonnet-4-20250514", ... }
 *   - Response SSE:  message_start → message.usage.input_tokens
 *                    message_delta → usage.output_tokens
 *
 * OpenAI Chat Completions API:
 *   - Request body:  { "model": "gpt-4o", ... }
 *   - Response SSE:  final chunk → usage.prompt_tokens / completion_tokens
 */

import type { TrafficRecord } from '../stores/httpTrafficStore';

// ── Types ──────────────────────────────────────────────────────────

export interface TokenUsage {
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  /** Total tokens (input + output + cache write; cache read is discounted) */
  totalTokens: number;
}

export interface ExchangeUsage {
  exchangeId: string;
  modelId: string | null;
  provider: 'anthropic' | 'openai' | 'unknown';
  usage: TokenUsage | null;
  /** Whether usage was successfully parsed (false = parsing failed or not an LLM API call) */
  isLlmCall: boolean;
  /** Ephemeral cache TTL in seconds, extracted from the Anthropic request body.
   *  null = not set / not an Anthropic call / couldn't parse. */
  cacheTtl: number | null;
  /** Whether the URL is a batch-processing endpoint. */
  isBatch: boolean;
}

// ── Public API ─────────────────────────────────────────────────────

/**
 * Extract usage + model info from a single exchange.
 *
 * Returns `null` usage when the exchange does not appear to be an LLM
 * API call (wrong URL, unparseable body, etc.).
 */
export function parseExchangeUsage(rec: TrafficRecord): ExchangeUsage {
  const ex = rec.exchange;

  // Quick URL gate — only process known LLM API endpoints.
  const isAnthropic = ex.url.includes('/v1/messages');
  const isOpenAi = ex.url.includes('/chat/completions');
  const isBatch = ex.url.includes('/batches') || ex.url.includes('/batch');

  if (!isAnthropic && !isOpenAi) {
    return {
      exchangeId: ex.exchangeId,
      modelId: null,
      provider: 'unknown',
      usage: null,
      isLlmCall: false,
      cacheTtl: null,
      isBatch: false,
    };
  }

  const provider: 'anthropic' | 'openai' = isAnthropic ? 'anthropic' : 'openai';

  // Extract model from request body.
  const modelId = extractModelId(ex.reqBody);

  // Extract cache TTL from Anthropic request body.
  const cacheTtl = isAnthropic ? extractCacheTtl(ex.reqBody) : null;

  // Extract usage from response body.
  let usage: TokenUsage | null = null;
  if (provider === 'anthropic') {
    usage = parseAnthropicUsage(ex.respBody);
  } else {
    usage = parseOpenAiUsage(ex.respBody);
  }

  return {
    exchangeId: ex.exchangeId,
    modelId,
    provider,
    usage,
    isLlmCall: true,
    cacheTtl,
    isBatch,
  };
}

/**
 * Batch-parse usage for all exchanges.
 */
export function parseAllExchangeUsages(
  records: TrafficRecord[],
): ExchangeUsage[] {
  return records.map(parseExchangeUsage);
}

// ── Model extraction ───────────────────────────────────────────────

function extractModelId(reqBody: number[]): string | null {
  if (!reqBody || reqBody.length === 0) return null;
  try {
    const json = JSON.parse(new TextDecoder().decode(new Uint8Array(reqBody)));
    const model = json?.model;
    return typeof model === 'string' ? model : null;
  } catch {
    return null;
  }
}

/** Extract ephemeral cache TTL from an Anthropic request body. */
function extractCacheTtl(reqBody: number[]): number | null {
  if (!reqBody || reqBody.length === 0) return null;
  try {
    const json = JSON.parse(new TextDecoder().decode(new Uint8Array(reqBody)));
    // Anthropic ephemeral_cache: { ttl: <seconds> }
    const ec = json?.ephemeral_cache;
    if (ec && typeof ec.ttl === 'number') return ec.ttl;
    // Also check cache_control for prompt caching beta.
    // Format: { "type": "ephemeral", "ttl": <seconds> }
    const cc = json?.cache_control;
    if (cc && typeof cc.ttl === 'number') return cc.ttl;
    return null;
  } catch {
    return null;
  }
}

// ── Anthropic usage parser ─────────────────────────────────────────

function parseAnthropicUsage(respBody: number[]): TokenUsage | null {
  if (!respBody || respBody.length === 0) return null;

  const text = new TextDecoder().decode(new Uint8Array(respBody));

  // Try full-body JSON first (non-streaming response).
  try {
    const json = JSON.parse(text);
    const u = json?.usage;
    if (u) {
      return normalizeAnthropicUsage(u.input_tokens, u.output_tokens, u.cache_creation_input_tokens, u.cache_read_input_tokens);
    }
  } catch {
    // Not a single JSON — try SSE streaming format.
  }

  // Parse SSE stream to find usage events.
  let inputTokens = 0;
  let outputTokens = 0;
  let cacheWriteTokens = 0;
  let cacheReadTokens = 0;

  for (const evt of parseSse(text)) {
    const d = evt.data as Record<string, unknown> | undefined;
    if (!d) continue;

    switch (d.type) {
      case 'message_start': {
        const msg = d.message as Record<string, unknown> | undefined;
        const usage = msg?.usage as Record<string, number> | undefined;
        if (usage) {
          inputTokens = usage.input_tokens ?? 0;
          cacheWriteTokens = usage.cache_creation_input_tokens ?? 0;
          cacheReadTokens = usage.cache_read_input_tokens ?? 0;
        }
        break;
      }
      case 'message_delta': {
        const usage = d.usage as Record<string, number> | undefined;
        if (usage) {
          outputTokens = usage.output_tokens ?? 0;
        }
        break;
      }
    }
  }

  if (inputTokens === 0 && outputTokens === 0) return null;
  return normalizeAnthropicUsage(inputTokens, outputTokens, cacheWriteTokens, cacheReadTokens);
}

function normalizeAnthropicUsage(
  input: number,
  output: number,
  cacheWrite: number,
  cacheRead: number,
): TokenUsage {
  return {
    inputTokens: input,
    outputTokens: output,
    cacheReadTokens: cacheRead,
    cacheWriteTokens: cacheWrite,
    totalTokens: input + output + cacheWrite,
  };
}

// ── OpenAI usage parser ────────────────────────────────────────────

function parseOpenAiUsage(respBody: number[]): TokenUsage | null {
  if (!respBody || respBody.length === 0) return null;

  const text = new TextDecoder().decode(new Uint8Array(respBody));

  // Try full-body JSON first.
  try {
    const json = JSON.parse(text);
    const u = json?.usage;
    if (u) {
      const promptCacheHit = u.prompt_tokens_details?.cached_tokens ?? 0;
      return {
        inputTokens: (u.prompt_tokens ?? 0) - promptCacheHit,
        outputTokens: u.completion_tokens ?? 0,
        cacheReadTokens: promptCacheHit,
        cacheWriteTokens: 0,
        totalTokens: (u.prompt_tokens ?? 0) + (u.completion_tokens ?? 0),
      };
    }
  } catch {
    // Not a single JSON — try SSE.
  }

  // Parse SSE stream.
  let promptTokens = 0;
  let completionTokens = 0;
  let cachedTokens = 0;

  for (const evt of parseSse(text)) {
    const d = evt.data as Record<string, unknown> | undefined;
    if (!d) continue;

    // OpenAI SSE chunks: each "data:" line is a JSON with optional "usage"
    const choices = d.choices as Array<Record<string, unknown>> | undefined;
    const usage = d.usage as Record<string, unknown> | undefined;
    if (usage) {
      promptTokens = (usage.prompt_tokens as number) ?? promptTokens;
      completionTokens = (usage.completion_tokens as number) ?? completionTokens;
      const details = usage.prompt_tokens_details as Record<string, number> | undefined;
      cachedTokens = details?.cached_tokens ?? cachedTokens;
    }
    // Ignore choices — we only care about usage.
    void choices;
  }

  if (promptTokens === 0 && completionTokens === 0) return null;
  return {
    inputTokens: promptTokens - cachedTokens,
    outputTokens: completionTokens,
    cacheReadTokens: cachedTokens,
    cacheWriteTokens: 0,
    totalTokens: promptTokens + completionTokens,
  };
}

// ── SSE line parser (shared) ───────────────────────────────────────

interface SseEvent {
  event: string;
  data: unknown;
}

function parseSse(raw: string): SseEvent[] {
  const events: SseEvent[] = [];
  let event = '';
  let data = '';

  for (const line of raw.split('\n')) {
    if (line.startsWith('event: ')) {
      event = line.slice(7).trim();
    } else if (line.startsWith('data: ')) {
      data = line.slice(6);
    } else if (line.trim() === '' && data) {
      try {
        events.push({ event, data: JSON.parse(data) });
      } catch {
        // skip
      }
      event = '';
      data = '';
    }
  }

  // Flush last event if no trailing newline.
  if (data) {
    try {
      events.push({ event, data: JSON.parse(data) });
    } catch {
      // skip
    }
  }

  return events;
}
