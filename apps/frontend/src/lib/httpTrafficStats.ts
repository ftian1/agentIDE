/**
 * HTTP Traffic Statistics — derives aggregated dashboard data from
 * the raw {@link TrafficRecord} array.
 *
 * All functions are pure: they take the exchange list and return
 * computed stats.  Components should wrap calls in `useMemo`.
 */

import type { TrafficRecord } from '../stores/httpTrafficStore';
import { parseAllExchangeUsages, type TokenUsage } from './usageParser';
import { computeCost, type CostContext, type CostBreakdown } from './pricingEngine';

// ── Types ──────────────────────────────────────────────────────────

export interface DashboardStats {
  // Summary
  total: number;
  llmCalls: number;
  avgLatency: number;
  p95Latency: number;
  errorRate: number;

  // Tokens
  totalInputTokens: number;
  totalOutputTokens: number;
  totalCacheReadTokens: number;
  totalCacheWriteTokens: number;

  // Cost
  totalCost: number;
  avgCostPerCall: number;

  // Distributions
  statusDistribution: { name: string; value: number; color: string }[];
  latencyHistogram: { binStart: number; binEnd: number; label: string; count: number }[];
  requestsOverTime: { time: number; count: number }[];
  methodDistribution: { method: string; count: number; color: string }[];
  topHosts: { host: string; count: number }[];

  // Cost by day (for the monthly spend chart)
  dailyCost: { date: string; cost: number; calls: number }[];

  // Tokens by day (for the stacked token chart)
  dailyTokens: {
    date: string;
    inputTokens: number;
    outputTokens: number;
    cacheReadTokens: number;
    cacheWriteTokens: number;
  }[];

  // Cost by model
  costByModel: { modelId: string; label: string; cost: number; calls: number }[];

  // Token breakdown
  tokenBreakdown: {
    inputTokens: number;
    outputTokens: number;
    cacheReadTokens: number;
    cacheWriteTokens: number;
  };

  // Models seen
  models: string[];
}

// ── Helpers ─────────────────────────────────────────────────────────

export function mean(values: number[]): number {
  if (values.length === 0) return 0;
  return values.reduce((a, b) => a + b, 0) / values.length;
}

export function percentile(values: number[], p: number): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const idx = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.max(0, Math.min(idx, sorted.length - 1))];
}

// ── Main derivation ─────────────────────────────────────────────────

const HOUR_MS = 3_600_000;
const DAY_MS = 86_400_000;

export function computeDashboardStats(records: TrafficRecord[]): DashboardStats {
  // ── Parse usage for all LLM calls ─────────────────────────────
  const usages = parseAllExchangeUsages(records);

  // Build a quick lookup from exchangeId → TrafficRecord for context.
  const recordMap = new Map<string, TrafficRecord>();
  for (const rec of records) {
    recordMap.set(rec.exchange.exchangeId, rec);
  }

  // Build a map for O(1) lookup of cost by exchangeId.
  const costMap = new Map<string, CostBreakdown>();
  for (const u of usages) {
    if (u.usage) {
      const rec = recordMap.get(u.exchangeId);
      const ctx: CostContext = {
        inputTokens: u.usage.inputTokens,
        outputTokens: u.usage.outputTokens,
        cacheReadTokens: u.usage.cacheReadTokens,
        cacheWriteTokens: u.usage.cacheWriteTokens,
        startedAt: rec?.exchange.startedAt ?? 0,
        isBatch: u.isBatch,
        cacheTtl: u.cacheTtl,
        provider: u.provider,
      };
      costMap.set(u.exchangeId, computeCost(u.modelId, u.usage, ctx));
    }
  }

  // ── Summary metrics ─────────────────────────────────────────
  const total = records.length;
  const llmCalls = usages.filter((u) => u.isLlmCall).length;

  const durations = records
    .map((r) => r.exchange.durationMs)
    .filter((d) => d > 0);
  const avgLatency = Math.round(mean(durations));
  const p95Latency = Math.round(percentile(durations, 95));

  const errors = records.filter(
    (r) => r.exchange.status >= 400 || (r.exchange.status === 0 && r.exchange.durationMs > 0),
  ).length;
  const errorRate = total > 0 ? (errors / total) * 100 : 0;

  // ── Token totals ────────────────────────────────────────────
  let totalInputTokens = 0;
  let totalOutputTokens = 0;
  let totalCacheReadTokens = 0;
  let totalCacheWriteTokens = 0;

  for (const u of usages) {
    if (!u.usage) continue;
    totalInputTokens += u.usage.inputTokens;
    totalOutputTokens += u.usage.outputTokens;
    totalCacheReadTokens += u.usage.cacheReadTokens;
    totalCacheWriteTokens += u.usage.cacheWriteTokens;
  }

  // ── Cost totals ─────────────────────────────────────────────
  let totalCost = 0;
  for (const [, c] of costMap) {
    totalCost += c.totalCost;
  }
  const avgCostPerCall = llmCalls > 0 ? totalCost / llmCalls : 0;

  // ── Status distribution ─────────────────────────────────────
  const statusDist: DashboardStats['statusDistribution'] = [
    { name: '2xx', value: 0, color: '#4ade80' },
    { name: '3xx', value: 0, color: '#60a5fa' },
    { name: '4xx', value: 0, color: '#facc15' },
    { name: '5xx', value: 0, color: '#f87171' },
    { name: 'Other', value: 0, color: '#8b949e' },
  ];
  for (const r of records) {
    const s = r.exchange.status;
    if (s >= 200 && s < 300) statusDist[0].value++;
    else if (s >= 300 && s < 400) statusDist[1].value++;
    else if (s >= 400 && s < 500) statusDist[2].value++;
    else if (s >= 500) statusDist[3].value++;
    else statusDist[4].value++;
  }

  // ── Latency histogram ───────────────────────────────────────
  const latencyHistogram = buildLatencyHistogram(durations);

  // ── Requests over time ─────────────────────────────────────
  const requestsOverTime = buildTimeSeries(records);

  // ── Method distribution ─────────────────────────────────────
  const methodDist = buildMethodDist(records);

  // ── Top hosts ───────────────────────────────────────────────
  const topHosts = buildTopHosts(records);

  // ── Daily cost ──────────────────────────────────────────────
  const dailyCost = buildDailyCost(records, costMap);

  // ── Daily tokens ────────────────────────────────────────────
  const dailyTokens = buildDailyTokens(records, usages);

  // ── Cost by model ───────────────────────────────────────────
  const costByModel = buildCostByModel(costMap, usages);

  // ── Models ──────────────────────────────────────────────────
  const models = [
    ...new Set(
      usages.filter((u) => u.modelId).map((u) => u.modelId!),
    ),
  ].sort();

  return {
    total,
    llmCalls,
    avgLatency,
    p95Latency,
    errorRate,
    totalInputTokens,
    totalOutputTokens,
    totalCacheReadTokens,
    totalCacheWriteTokens,
    totalCost,
    avgCostPerCall,
    statusDistribution: statusDist.filter((s) => s.value > 0),
    latencyHistogram,
    requestsOverTime,
    methodDistribution: methodDist,
    topHosts,
    dailyCost,
    dailyTokens,
    costByModel,
    tokenBreakdown: {
      inputTokens: totalInputTokens,
      outputTokens: totalOutputTokens,
      cacheReadTokens: totalCacheReadTokens,
      cacheWriteTokens: totalCacheWriteTokens,
    },
    models,
  };
}

// ── Distribution builders ──────────────────────────────────────────

function buildLatencyHistogram(
  durations: number[],
): DashboardStats['latencyHistogram'] {
  if (durations.length === 0) return [];

  const bins = [0, 50, 100, 250, 500, 1000, 2500, 5000, 10000, 30000, Infinity];
  const counts = new Array(bins.length - 1).fill(0);

  for (const d of durations) {
    for (let i = 0; i < bins.length - 1; i++) {
      if (d >= bins[i] && d < bins[i + 1]) {
        counts[i]++;
        break;
      }
    }
  }

  return bins.slice(0, -1).map((start, i) => {
    const end = bins[i + 1];
    const endLabel = end === Infinity ? '+' : `-${end}ms`;
    return {
      binStart: start,
      binEnd: end === Infinity ? 999_999 : end,
      label: end === Infinity ? `${start}ms+` : `${start}${endLabel}`,
      count: counts[i],
    };
  });
}

function buildTimeSeries(
  records: TrafficRecord[],
): DashboardStats['requestsOverTime'] {
  // Only compute time series when at least one exchange has a valid timestamp.
  const withTs = records.filter((r) => r.exchange.startedAt > 0);
  if (withTs.length === 0) return [];

  const times = withTs.map((r) => r.exchange.startedAt);
  const minTime = Math.min(...times);
  const maxTime = Math.max(...times);
  const range = maxTime - minTime;

  // Adaptive bucket size.
  let bucketMs: number;
  if (range <= 60_000) bucketMs = 10_000; // 10 sec
  else if (range <= HOUR_MS) bucketMs = 60_000; // 1 min
  else if (range <= 6 * HOUR_MS) bucketMs = 300_000; // 5 min
  else bucketMs = 3_600_000; // 1 hour

  const buckets = new Map<number, number>();
  for (const r of withTs) {
    const bucket = Math.floor(r.exchange.startedAt / bucketMs) * bucketMs;
    buckets.set(bucket, (buckets.get(bucket) || 0) + 1);
  }

  // Fill gaps.
  const result: { time: number; count: number }[] = [];
  for (let t = Math.floor(minTime / bucketMs) * bucketMs; t <= maxTime; t += bucketMs) {
    result.push({ time: t, count: buckets.get(t) || 0 });
  }

  return result;
}

function buildMethodDist(
  records: TrafficRecord[],
): DashboardStats['methodDistribution'] {
  const METHOD_COLORS: Record<string, string> = {
    GET: '#60a5fa',
    POST: '#4ade80',
    DELETE: '#f87171',
    PATCH: '#facc15',
    PUT: '#c084fc',
  };

  const counts = new Map<string, number>();
  for (const r of records) {
    const m = r.exchange.method || 'UNKNOWN';
    counts.set(m, (counts.get(m) || 0) + 1);
  }

  return Array.from(counts.entries())
    .sort((a, b) => b[1] - a[1])
    .map(([method, count]) => ({
      method,
      count,
      color: METHOD_COLORS[method] || '#8b949e',
    }));
}

function buildTopHosts(
  records: TrafficRecord[],
): DashboardStats['topHosts'] {
  const counts = new Map<string, number>();
  for (const r of records) {
    const h = r.exchange.host || '(unknown)';
    counts.set(h, (counts.get(h) || 0) + 1);
  }

  return Array.from(counts.entries())
    .sort((a, b) => b[1] - a[1])
    .slice(0, 10)
    .map(([host, count]) => ({ host, count }));
}

// ── Cost / token time series ───────────────────────────────────────

function buildDailyCost(
  records: TrafficRecord[],
  costMap: Map<string, CostBreakdown>,
): DashboardStats['dailyCost'] {
  const dailyMap = new Map<string, { cost: number; calls: number }>();

  for (const r of records) {
    const ts = r.exchange.startedAt || 0;
    let date: string;
    if (ts > 0) {
      date = new Date(ts).toISOString().slice(0, 10); // YYYY-MM-DD
    } else {
      // DB-loaded data without startedAt — use "unknown" bucket.
      date = '(unknown)';
    }

    const existing = dailyMap.get(date) || { cost: 0, calls: 0 };
    const c = costMap.get(r.exchange.exchangeId);
    existing.cost += c?.totalCost ?? 0;
    existing.calls += 1;
    dailyMap.set(date, existing);
  }

  return Array.from(dailyMap.entries())
    .sort((a, b) => a[0].localeCompare(b[0]))
    .map(([date, v]) => ({
      date,
      cost: Math.round(v.cost * 10000) / 10000,
      calls: v.calls,
    }));
}

function buildDailyTokens(
  records: TrafficRecord[],
  usages: ReturnType<typeof parseAllExchangeUsages>,
): DashboardStats['dailyTokens'] {
  const dailyMap = new Map<
    string,
    { input: number; output: number; cacheRead: number; cacheWrite: number }
  >();

  const usageMap = new Map<string, TokenUsage | null>();
  for (const u of usages) {
    usageMap.set(u.exchangeId, u.usage);
  }

  for (const r of records) {
    const ts = r.exchange.startedAt || 0;
    let date: string;
    if (ts > 0) {
      date = new Date(ts).toISOString().slice(0, 10);
    } else {
      date = '(unknown)';
    }

    const existing = dailyMap.get(date) || {
      input: 0,
      output: 0,
      cacheRead: 0,
      cacheWrite: 0,
    };

    const u = usageMap.get(r.exchange.exchangeId);
    if (u) {
      existing.input += u.inputTokens;
      existing.output += u.outputTokens;
      existing.cacheRead += u.cacheReadTokens;
      existing.cacheWrite += u.cacheWriteTokens;
    }
    dailyMap.set(date, existing);
  }

  return Array.from(dailyMap.entries())
    .sort((a, b) => a[0].localeCompare(b[0]))
    .map(([date, v]) => ({
      date,
      inputTokens: v.input,
      outputTokens: v.output,
      cacheReadTokens: v.cacheRead,
      cacheWriteTokens: v.cacheWrite,
    }));
}

function buildCostByModel(
  costMap: Map<string, CostBreakdown>,
  usages: ReturnType<typeof parseAllExchangeUsages>,
): DashboardStats['costByModel'] {
  const modelMap = new Map<string, { cost: number; calls: number; label: string }>();

  for (const u of usages) {
    if (!u.modelId) continue;
    const c = costMap.get(u.exchangeId);
    if (!c) continue;

    const key = u.modelId;
    const existing = modelMap.get(key) || { cost: 0, calls: 0, label: '' };
    existing.cost += c.totalCost;
    existing.calls += 1;
    existing.label = c.label || u.modelId;
    modelMap.set(key, existing);
  }

  return Array.from(modelMap.entries())
    .sort((a, b) => b[1].cost - a[1].cost)
    .map(([modelId, v]) => ({
      modelId,
      label: v.label,
      cost: Math.round(v.cost * 10000) / 10000,
      calls: v.calls,
    }));
}
