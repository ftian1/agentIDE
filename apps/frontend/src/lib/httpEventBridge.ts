/**
 * HTTP Event Bridge — subscribes to `http:traffic` Tauri events and feeds
 * parsed Anthropic API exchanges into the agent store as structured blocks
 * for the visual interaction layer.
 *
 * This replaces the `--output-format stream-json` dependency: the MITM proxy
 * captures every API request/response from the CLI, and the SSE response body
 * contains the same structured data the CLI would otherwise emit as NDJSON.
 *
 * Initialise once at app startup via {@link initHttpEventBridge}.
 */
import { listen } from '@tauri-apps/api/event';
import { parseHttpExchange } from './httpEventParser';
import { useAgentStore } from '../stores/agentStore';
import { log } from './debugLog';

/** Per-session set of exchange IDs already ingested (dedup). */
const seenExchanges = new Map<string, Set<string>>();

/**
 * Initialise the HTTP → AgentEvent bridge.
 *
 * Should be called once at app startup, after the agent store and
 * terminal API are ready.
 */
export function initHttpEventBridge() {
  listen<{
    session_id: string;
    connection_id: string;
    seq: number;
    exchange: {
      exchangeId: string;
      method: string;
      url: string;
      host: string;
      reqHeaders: Record<string, string>;
      reqBody: number[];
      status: number;
      respHeaders: Record<string, string>;
      respBody: number[];
      startedAt: number;
      durationMs: number;
      truncated: boolean;
    };
  }>('http:traffic', (event) => {
    try {
      const { session_id, seq, exchange } = event.payload;

      // Only process POST requests to /v1/messages (the actual conversation).
      if (
        exchange.method !== 'POST' ||
        !exchange.url.includes('/v1/messages')
      ) {
        return;
      }

      // Dedup by session + exchangeId
      let seen = seenExchanges.get(session_id);
      if (!seen) {
        seen = new Set();
        seenExchanges.set(session_id, seen);
      }
      if (seen.has(exchange.exchangeId)) return;
      seen.add(exchange.exchangeId);

      // Skip empty responses (e.g. network errors, truncated before data).
      if (!exchange.respBody || exchange.respBody.length === 0) return;

      const reqBody = new Uint8Array(exchange.reqBody);
      const respBody = new Uint8Array(exchange.respBody);

      const blocks = parseHttpExchange(reqBody, respBody, session_id, seq);

      if (blocks.length === 0) return;

      log(
        'agent',
        `httpBridge: session=${session_id.slice(0, 8)} seq=${seq} blocks=${blocks.length}`,
      );

      // Feed blocks into the agent store.
      const store = useAgentStore.getState();
      for (const block of blocks) {
        store._appendBlockFromHttp(session_id, block);
      }
    } catch (err) {
      // Never let a parse failure break the event loop.
      log('agent', `httpBridge parse error: ${err}`);
    }
  });
}
