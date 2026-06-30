/**
 * Built-in pricing — loaded at build time from the repo-root pricing.json
 * and bundled into the JS payload.  This is the ultimate fallback when
 * both the remote fetch and localStorage cache are unavailable.
 *
 * The single source of truth is `/pricing.json` at the repo root.
 * Update that file to change pricing; the build will embed it automatically.
 *
 * Pricing units: USD per 1 000 000 (1M) tokens.
 *
 * Schema v4: providers → { label, overrides?, models: { modelId → entry } }.
 * On load, provider-level overrides are prepended to each model's override
 * list and the result is flattened into the runtime ProviderPricingMap.
 */

// @ts-expect-error Vite resolves @repo alias to repo root; JSON import is allowed.
import pricingData from '@repo/pricing.json';

// ── Types ───────────────────────────────────────────────────────────

export interface ModelPricing {
  label: string;
  inputPrice: number;
  outputPrice: number;
  cacheReadPrice: number;
  cacheWritePrice: number;
}

/** A single conditional pricing override. */
export interface PriceOverride {
  name: string;
  /** Predicate: all fields must match for the override to apply (AND logic). */
  when: Record<string, unknown>;
  /** Explicit price overrides — only the fields present are changed. */
  inputPrice?: number;
  outputPrice?: number;
  cacheReadPrice?: number;
  cacheWritePrice?: number;
  /** Multiply ALL base prices before applying explicit overrides. */
  multiplier?: number;
}

/** Raw model entry as it appears in pricing.json (v1 or v2). */
export interface PricingEntry {
  label: string;
  inputPrice: number;
  outputPrice: number;
  cacheReadPrice: number;
  cacheWritePrice: number;
  overrides?: PriceOverride[];
}

/** Flat runtime lookup: modelId → PricingEntry (with provider overrides already merged). */
export type ProviderPricingMap = Record<string, PricingEntry>;

/** Provider-level metadata for UI display. */
export interface ProviderGroup {
  id: string;
  label: string;
  modelIds: string[];
}

// ── Validation ──────────────────────────────────────────────────────

/** Parse a raw override entry from JSON. */
function parseOverride(ov: Record<string, unknown>, i: number): PriceOverride {
  return {
    name: (ov.name as string) ?? `override-${i}`,
    when: (ov.when as Record<string, unknown>) ?? {},
    inputPrice: ov.inputPrice as number | undefined,
    outputPrice: ov.outputPrice as number | undefined,
    cacheReadPrice: ov.cacheReadPrice as number | undefined,
    cacheWritePrice: ov.cacheWritePrice as number | undefined,
    multiplier: ov.multiplier as number | undefined,
  };
}

/** Parse a model entry and return a PricingEntry, or null if invalid / zero-price. */
function parseModelEntry(m: Record<string, unknown>): PricingEntry | null {
  if (!m || typeof m.label !== 'string') return null;
  if (
    (m.inputPrice as number) === 0 &&
    (m.outputPrice as number) === 0 &&
    (m.cacheReadPrice as number) === 0 &&
    (m.cacheWritePrice as number) === 0
  )
    return null; // skip zero-price placeholder entries

  const entry: PricingEntry = {
    label: m.label as string,
    inputPrice: typeof m.inputPrice === 'number' ? m.inputPrice : 0,
    outputPrice: typeof m.outputPrice === 'number' ? m.outputPrice : 0,
    cacheReadPrice: typeof m.cacheReadPrice === 'number' ? m.cacheReadPrice : 0,
    cacheWritePrice: typeof m.cacheWritePrice === 'number' ? m.cacheWritePrice : 0,
  };

  const rawOverrides = m.overrides as Array<Record<string, unknown>> | undefined;
  if (Array.isArray(rawOverrides) && rawOverrides.length > 0) {
    entry.overrides = rawOverrides.map(parseOverride);
  }

  return entry;
}

/**
 * Parse and flatten a pricing.json payload (v4 providers or v1–v3 flat models).
 * Exported so the runtime fetch path can reuse the same logic.
 */
export function parsePricingData(data: unknown): {
  models: ProviderPricingMap;
  providerGroups: ProviderGroup[];
} {
  const root = data as Record<string, unknown> | null;
  const models: ProviderPricingMap = {};
  const providerGroups: ProviderGroup[] = [];

  // ── v4+ schema: providers → { label, overrides?, models } ─────
  const providers = root?.providers as Record<string, unknown> | undefined;
  if (providers) {
    for (const [providerId, providerVal] of Object.entries(providers)) {
      const pv = providerVal as Record<string, unknown> | null;
      if (!pv || typeof pv.label !== 'string') continue;

      const providerLabel = pv.label as string;
      const rawModels = pv.models as Record<string, unknown> | undefined;
      if (!rawModels) continue;

      // Parse provider-level overrides (e.g. DeepSeek peak pricing).
      const rawProvOverrides = pv.overrides as Array<Record<string, unknown>> | undefined;
      const provOverrides: PriceOverride[] = [];
      if (Array.isArray(rawProvOverrides) && rawProvOverrides.length > 0) {
        provOverrides.push(...rawProvOverrides.map(parseOverride));
      }

      const modelIds: string[] = [];

      for (const [modelId, modelVal] of Object.entries(rawModels)) {
        const entry = parseModelEntry(modelVal as Record<string, unknown>);
        if (!entry) continue;

        // Prepend provider overrides → model's own overrides.
        if (provOverrides.length > 0) {
          entry.overrides = [...provOverrides, ...(entry.overrides ?? [])];
        }

        models[modelId] = entry;
        modelIds.push(modelId);
      }

      if (modelIds.length > 0) {
        providerGroups.push({ id: providerId, label: providerLabel, modelIds });
      }
    }

    if (Object.keys(models).length === 0) {
      throw new Error(
        'pricing.json: invalid schema. Expected { providers: { ... } } with model entries.',
      );
    }

    return { models, providerGroups };
  }

  // ── v1–v3 fallback: flat { models: { modelId → entry } } ────
  const flatModels = root?.models as Record<string, unknown> | undefined;
  if (!flatModels) {
    throw new Error(
      'pricing.json: invalid schema. Expected { providers: { ... } } or { models: { ... } }',
    );
  }

  for (const [modelId, modelVal] of Object.entries(flatModels)) {
    const entry = parseModelEntry(modelVal as Record<string, unknown>);
    if (entry) models[modelId] = entry;
  }

  return { models, providerGroups: [] };
}

const extracted = parsePricingData(pricingData);

/** Built-in pricing extracted from the bundled pricing.json (flat runtime map). */
export const DEFAULT_PRICING: ProviderPricingMap = extracted.models;

/** Provider groups for UI display (provider → label + model IDs). */
export const DEFAULT_PROVIDER_GROUPS: ProviderGroup[] = extracted.providerGroups;

/** Pricing format version — must match the cache schema. */
export const PRICING_FORMAT_VERSION = 3;
