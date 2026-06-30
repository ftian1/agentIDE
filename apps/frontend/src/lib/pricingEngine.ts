/**
 * Pricing Engine — computes USD cost from token usage using provider-specific
 * pricing tables.  Supports conditional pricing overrides (v2 schema) and
 * over-the-air updates from a GitHub-hosted pricing.json.
 *
 * Architecture:
 *   1. At build time, `/pricing.json` is bundled into the JS payload
 *      (via `pricingDefaults.ts`).  This is the bedrock fallback.
 *   2. On init, check localStorage for a previously cached remote update.
 *   3. Background-fetch the latest pricing from the GitHub URL.
 *      On success → persist to localStorage, use as active pricing.
 *      On failure → continue with built-in or cached.
 *   4. When computing cost, evaluate conditional overrides against the
 *      exchange context (token counts, time, batch, cache TTL).
 *
 * Fallback chain:  remote fetch → localStorage cache → built-in (bundled)
 */

import {
  DEFAULT_PRICING,
  PRICING_FORMAT_VERSION,
  type PricingEntry,
  type PriceOverride,
  type ProviderPricingMap,
} from './pricingDefaults';
import type { TokenUsage } from './usageParser';

// ── Config ──────────────────────────────────────────────────────────

const DEFAULT_PRICING_URL =
  'https://raw.githubusercontent.com/ftian1/agentIDE/refs/heads/main/pricing.json';

const CACHE_KEY = 'ai_pricing_cache_v3';
const CACHE_TTL_MS = 24 * 60 * 60 * 1000;
const FETCH_TIMEOUT_MS = 10_000;

// ── Types ───────────────────────────────────────────────────────────

export interface CostBreakdown {
  inputCost: number;
  outputCost: number;
  cacheReadCost: number;
  cacheWriteCost: number;
  totalCost: number;
  modelId: string | null;
  /** The resolved pricing entry used (after override evaluation). */
  label: string | null;
  /** Name of the matched override, if any. */
  overrideName: string | null;
}

/**
 * Context available for evaluating conditional pricing overrides.
 *
 * All fields are optional — when missing, conditions that depend on
 * that field silently fail to match (i.e. the override is skipped).
 */
export interface CostContext {
  inputTokens?: number;
  outputTokens?: number;
  cacheReadTokens?: number;
  cacheWriteTokens?: number;
  /** Epoch milliseconds — 0 or undefined means "unknown". */
  startedAt?: number;
  /** Detected from URL path. */
  isBatch?: boolean;
  /** Ephemeral cache TTL in seconds, or undefined if not set. */
  cacheTtl?: number | null;
  /** Provider string for provider-specific conditions. */
  provider?: string | null;
}

interface PricingCache {
  version: number;
  fetchedAt: number;
  url: string;
  models: ProviderPricingMap;
}

// ── State ───────────────────────────────────────────────────────────

let activePricing: ProviderPricingMap = { ...DEFAULT_PRICING };
let initialized = false;

// ── Public API ──────────────────────────────────────────────────────

export async function initPricingEngine(pricingUrl?: string): Promise<void> {
  if (initialized) return;

  const url = pricingUrl || DEFAULT_PRICING_URL;

  const cached = loadCachedPricing();
  if (cached) {
    activePricing = { ...DEFAULT_PRICING, ...cached };
  }

  fetchAndCachePricing(url)
    .then((ok) => {
      if (ok) {
        const fresh = loadCachedPricing();
        if (fresh) activePricing = { ...DEFAULT_PRICING, ...fresh };
      }
    })
    .catch(() => {});

  initialized = true;
}

/**
 * Compute the USD cost for a single exchange.
 *
 * When `ctx` is provided, conditional overrides are evaluated against it
 * (first match wins).  Without context, base prices are used.
 */
export function computeCost(
  modelId: string | null,
  usage: TokenUsage | null,
  ctx?: CostContext,
): CostBreakdown {
  const zero: CostBreakdown = {
    inputCost: 0,
    outputCost: 0,
    cacheReadCost: 0,
    cacheWriteCost: 0,
    totalCost: 0,
    modelId,
    label: null,
    overrideName: null,
  };

  if (!modelId || !usage) return zero;

  const entry = findPricing(modelId);
  if (!entry) return zero;

  // Resolve effective prices (base + overrides evaluated against ctx).
  const effective = resolveEffectivePricing(entry, ctx);

  const inputCost = (usage.inputTokens / 1_000_000) * effective.inputPrice;
  const outputCost = (usage.outputTokens / 1_000_000) * effective.outputPrice;
  const cacheReadCost =
    (usage.cacheReadTokens / 1_000_000) * (effective.cacheReadPrice || 0);
  const cacheWriteCost =
    (usage.cacheWriteTokens / 1_000_000) * (effective.cacheWritePrice || 0);

  return {
    inputCost: roundCents(inputCost),
    outputCost: roundCents(outputCost),
    cacheReadCost: roundCents(cacheReadCost),
    cacheWriteCost: roundCents(cacheWriteCost),
    totalCost: roundCents(
      inputCost + outputCost + cacheReadCost + cacheWriteCost,
    ),
    modelId,
    label: effective.label,
    overrideName: effective.overrideName,
  };
}

export function getActivePricing(): ProviderPricingMap {
  return activePricing;
}

export async function refreshPricing(pricingUrl?: string): Promise<boolean> {
  const url = pricingUrl || DEFAULT_PRICING_URL;
  const ok = await fetchAndCachePricing(url);
  if (ok) {
    const fresh = loadCachedPricing();
    if (fresh) activePricing = { ...DEFAULT_PRICING, ...fresh };
  }
  return ok;
}

// ── Pricing lookup ─────────────────────────────────────────────────

function findPricing(modelId: string): PricingEntry | null {
  const key = modelId.toLowerCase();

  if (activePricing[key]) return activePricing[key];

  // Longest-prefix match.
  const keys = Object.keys(activePricing).sort((a, b) => b.length - a.length);
  for (const k of keys) {
    if (key.startsWith(k)) return activePricing[k];
  }

  return null;
}

// ── Override resolution ────────────────────────────────────────────

interface ResolvedPricing {
  label: string;
  inputPrice: number;
  outputPrice: number;
  cacheReadPrice: number;
  cacheWritePrice: number;
  overrideName: string | null;
}

function resolveEffectivePricing(
  entry: PricingEntry,
  ctx?: CostContext,
): ResolvedPricing {
  const base = {
    label: entry.label,
    inputPrice: entry.inputPrice,
    outputPrice: entry.outputPrice,
    cacheReadPrice: entry.cacheReadPrice,
    cacheWritePrice: entry.cacheWritePrice,
    overrideName: null as string | null,
  };

  if (!ctx || !entry.overrides || entry.overrides.length === 0) return base;

  for (const ov of entry.overrides) {
    if (!matchPredicate(ov.when, ctx)) continue;

    // Apply multiplier first, then explicit overrides.
    let prices = { ...base };
    if (ov.multiplier) {
      prices.inputPrice = roundCents(prices.inputPrice * ov.multiplier);
      prices.outputPrice = roundCents(prices.outputPrice * ov.multiplier);
      prices.cacheReadPrice = roundCents(prices.cacheReadPrice * ov.multiplier);
      prices.cacheWritePrice = roundCents(
        prices.cacheWritePrice * ov.multiplier,
      );
    }
    if (ov.inputPrice !== undefined) prices.inputPrice = ov.inputPrice;
    if (ov.outputPrice !== undefined) prices.outputPrice = ov.outputPrice;
    if (ov.cacheReadPrice !== undefined)
      prices.cacheReadPrice = ov.cacheReadPrice;
    if (ov.cacheWritePrice !== undefined)
      prices.cacheWritePrice = ov.cacheWritePrice;

    prices.overrideName = ov.name;
    return prices;
  }

  return base;
}

// ── Predicate matching ─────────────────────────────────────────────

type NumericOp = { gt?: number; gte?: number; lt?: number; lte?: number; eq?: number };
type InOp = { in?: number[] };
type TimeOfDayOp = { gte?: string; lt?: string };

function matchPredicate(
  when: Record<string, unknown>,
  ctx: CostContext,
): boolean {
  for (const [key, rawOp] of Object.entries(when)) {
    if (!matchField(key, rawOp as Record<string, unknown>, ctx)) return false;
  }
  return true;
}

function matchField(
  key: string,
  op: Record<string, unknown>,
  ctx: CostContext,
): boolean {
  switch (key) {
    // ── Numeric fields ──────────────────────────────────────
    case 'inputTokens':
    case 'outputTokens':
    case 'cacheReadTokens':
    case 'cacheWriteTokens':
    case 'totalTokens': {
      const numOp = op as NumericOp;
      const value = numericField(key, ctx);
      if (value === undefined) return false;
      return matchNumeric(value, numOp);
    }

    // ── Time fields ─────────────────────────────────────────
    case 'dayOfWeek': {
      const inOp = op as InOp;
      if (ctx.startedAt === undefined || ctx.startedAt === 0) return false;
      const dow = new Date(ctx.startedAt).getDay(); // 0=Sun … 6=Sat
      return inOp.in !== undefined ? inOp.in.includes(dow) : false;
    }
    case 'timeOfDay': {
      const tOp = op as TimeOfDayOp;
      if (ctx.startedAt === undefined || ctx.startedAt === 0) return false;
      const d = new Date(ctx.startedAt);
      const minutes = d.getHours() * 60 + d.getMinutes();
      const gte = timeStrToMinutes(tOp.gte);
      const lt = timeStrToMinutes(tOp.lt);
      if (gte !== undefined && minutes < gte) return false;
      if (lt !== undefined && minutes >= lt) return false;
      return true;
    }

    // ── Boolean fields ──────────────────────────────────────
    case 'isBatch': {
      const eqOp = op as { eq?: boolean };
      return ctx.isBatch === !!eqOp.eq;
    }

    // ── Cache TTL ───────────────────────────────────────────
    case 'cacheTtl': {
      const numOp = op as NumericOp;
      const value = ctx.cacheTtl ?? undefined;
      if (value === undefined) return false;
      return matchNumeric(value, numOp);
    }

    // ── Provider ────────────────────────────────────────────
    case 'provider': {
      const eqOp = op as { eq?: string };
      return (ctx.provider ?? '') === (eqOp.eq ?? '');
    }

    default:
      // Unknown condition key → don't match (conservative).
      return false;
  }
}

// ── Helpers ────────────────────────────────────────────────────────

function numericField(
  key: string,
  ctx: CostContext,
): number | undefined {
  switch (key) {
    case 'inputTokens':
      return ctx.inputTokens;
    case 'outputTokens':
      return ctx.outputTokens;
    case 'cacheReadTokens':
      return ctx.cacheReadTokens;
    case 'cacheWriteTokens':
      return ctx.cacheWriteTokens;
    case 'totalTokens':
      return (
        (ctx.inputTokens ?? 0) +
        (ctx.outputTokens ?? 0) +
        (ctx.cacheWriteTokens ?? 0)
      );
    default:
      return undefined;
  }
}

function matchNumeric(value: number, op: NumericOp): boolean {
  if (op.gt !== undefined && !(value > op.gt)) return false;
  if (op.gte !== undefined && !(value >= op.gte)) return false;
  if (op.lt !== undefined && !(value < op.lt)) return false;
  if (op.lte !== undefined && !(value <= op.lte)) return false;
  if (op.eq !== undefined && value !== op.eq) return false;
  return true;
}

function timeStrToMinutes(s?: string): number | undefined {
  if (!s) return undefined;
  const parts = s.split(':').map(Number);
  if (parts.length < 2 || isNaN(parts[0]) || isNaN(parts[1])) return undefined;
  return parts[0] * 60 + parts[1];
}

function roundCents(n: number): number {
  return Math.round(n * 10000) / 10000;
}

// ── Cache & fetch ─────────────────────────────────────────────────

function loadCachedPricing(): ProviderPricingMap | null {
  try {
    const raw = localStorage.getItem(CACHE_KEY);
    if (!raw) return null;
    const cache: PricingCache = JSON.parse(raw);
    if (cache.version !== PRICING_FORMAT_VERSION) return null;
    if (Date.now() - cache.fetchedAt > CACHE_TTL_MS) return null;
    return cache.models;
  } catch {
    return null;
  }
}

async function fetchAndCachePricing(url: string): Promise<boolean> {
  if (!url) return false;

  try {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), FETCH_TIMEOUT_MS);

    const resp = await fetch(url, {
      signal: controller.signal,
      headers: { Accept: 'application/json' },
    });
    clearTimeout(timeout);

    if (!resp.ok) return false;

    const json: unknown = await resp.json();
    const root = json as Record<string, unknown>;
    if (!root || !root.models) return false;

    const models = root.models as Record<string, unknown>;
    if (!models || Object.keys(models).length === 0) return false;

    // Validate model entries.
    const validated: ProviderPricingMap = {};
    for (const [key, val] of Object.entries(models)) {
      const m = val as Record<string, unknown> | null;
      if (!m || typeof m.label !== 'string') continue;
      if (
        (m.inputPrice as number) === 0 &&
        (m.outputPrice as number) === 0 &&
        (m.cacheReadPrice as number) === 0 &&
        (m.cacheWritePrice as number) === 0
      )
        continue;

      const entry: PricingEntry = {
        label: m.label as string,
        inputPrice: typeof m.inputPrice === 'number' ? m.inputPrice : 0,
        outputPrice: typeof m.outputPrice === 'number' ? m.outputPrice : 0,
        cacheReadPrice: typeof m.cacheReadPrice === 'number' ? m.cacheReadPrice : 0,
        cacheWritePrice: typeof m.cacheWritePrice === 'number' ? m.cacheWritePrice : 0,
      };

      const rawOverrides = m.overrides as Array<Record<string, unknown>> | undefined;
      if (Array.isArray(rawOverrides) && rawOverrides.length > 0) {
        entry.overrides = rawOverrides.map((ov, i) => ({
          name: (ov.name as string) ?? `override-${i}`,
          when: (ov.when as Record<string, unknown>) ?? {},
          inputPrice: ov.inputPrice as number | undefined,
          outputPrice: ov.outputPrice as number | undefined,
          cacheReadPrice: ov.cacheReadPrice as number | undefined,
          cacheWritePrice: ov.cacheWritePrice as number | undefined,
          multiplier: ov.multiplier as number | undefined,
        }));
      }

      validated[key] = entry;
    }

    if (Object.keys(validated).length === 0) return false;

    const cache: PricingCache = {
      version: PRICING_FORMAT_VERSION,
      fetchedAt: Date.now(),
      url,
      models: validated,
    };

    localStorage.setItem(CACHE_KEY, JSON.stringify(cache));
    return true;
  } catch {
    return false;
  }
}
