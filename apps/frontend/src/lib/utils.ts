/** General utility functions for the frontend. */

/** Format a timestamp (unix millis) to a human-readable string. */
export function formatTimestamp(ts: number): string {
  return new Date(ts).toLocaleString();
}

/** Format token count with commas. */
export function formatTokens(n: number): string {
  return n.toLocaleString();
}

/** Format cost in USD. */
export function formatCost(usd: number): string {
  return `$${usd.toFixed(2)}`;
}

/** Truncate a string to a maximum length with ellipsis. */
export function truncate(s: string, maxLen: number): string {
  return s.length <= maxLen ? s : s.slice(0, maxLen - 3) + '...';
}

/** Generate a short random ID (not cryptographically secure). */
export function shortId(): string {
  return Math.random().toString(36).substring(2, 10);
}
