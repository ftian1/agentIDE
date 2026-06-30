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
 * Supports both v1 (flat) and v2 (with conditional overrides) schemas.
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

export type ProviderPricingMap = Record<string, PricingEntry>;

// ── Validation ──────────────────────────────────────────────────────

function validateAndExtract(data: unknown): ProviderPricingMap {
  const root = data as Record<string, unknown> | null;
  if (!root || !root.models) {
    throw new Error(
      'pricing.json: invalid schema. Expected { version: 1|2, models: { ... } }',
    );
  }

  const models = root.models as Record<string, unknown>;
  const result: ProviderPricingMap = {};

  for (const [key, val] of Object.entries(models)) {
    const m = val as Record<string, unknown> | null;
    if (!m || typeof m.label !== 'string') continue;
    if (
      (m.inputPrice as number) === 0 &&
      (m.outputPrice as number) === 0 &&
      (m.cacheReadPrice as number) === 0 &&
      (m.cacheWritePrice as number) === 0
    )
      continue; // skip zero-price placeholder entries

    const entry: PricingEntry = {
      label: m.label as string,
      inputPrice: typeof m.inputPrice === 'number' ? m.inputPrice : 0,
      outputPrice: typeof m.outputPrice === 'number' ? m.outputPrice : 0,
      cacheReadPrice: typeof m.cacheReadPrice === 'number' ? m.cacheReadPrice : 0,
      cacheWritePrice: typeof m.cacheWritePrice === 'number' ? m.cacheWritePrice : 0,
    };

    // Parse v2 overrides if present.
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

    result[key] = entry;
  }

  return result;
}

/** Built-in pricing extracted from the bundled pricing.json. */
export const DEFAULT_PRICING: ProviderPricingMap = validateAndExtract(pricingData);

/** Pricing format version — must match pricing.json. */
export const PRICING_FORMAT_VERSION = 1;
