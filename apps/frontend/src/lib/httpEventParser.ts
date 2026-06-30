/**
 * HTTP Event Parser — converts Anthropic Messages API SSE responses into
 * structured {@link AgentBlock} items for the visual interaction layer.
 *
 * Also extracts tool_result content from the **next** request body so that
 * tool execution outputs (which the CLI captures internally, not via HTTP)
 * still appear in the visual layer.
 *
 * This replaces the need for `--output-format stream-json`: the proxy sees
 * the same structured data in a cleaner, CLI-independent format.
 */
import type { AgentBlock } from '../stores/agentStore';

// ── Types ──────────────────────────────────────────────────────────

/** A single SSE event from the Anthropic API stream. */
interface SseEvent {
  event: string;
  data: unknown;
}

/** Parsed content block accumulator (one per block in the stream). */
interface ContentBlock {
  index: number;
  type: string;
  name?: string;
  id?: string;
  text: string;
  toolInput: string;
  thinking: string;
}

// ── SSE Parser ─────────────────────────────────────────────────────

/**
 * Parse an Anthropic SSE response body into a list of {@link AgentBlock}.
 *
 * The Anthropic SSE format:
 * ```
 * event: message_start
 * data: {"type":"message_start","message":{...}}
 *
 * event: content_block_start
 * data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}
 *
 * event: content_block_delta
 * data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}
 *
 * event: content_block_stop
 * data: {"type":"content_block_stop","index":0}
 *
 * event: content_block_start
 * data: {"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"tool_1","name":"Bash","input":{}}}
 *
 * event: content_block_delta
 * data: {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"command\":\"ls\"}"}}
 *
 * event: message_delta
 * data: {"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":42}}
 *
 * event: message_stop
 * data: {"type":"message_stop"}
 * ```
 */
export function parseSseResponse(
  body: Uint8Array,
  sessionId: string,
  exchangeSeq: number,
): AgentBlock[] {
  const text = new TextDecoder().decode(body);
  const events = parseSse(text);
  const blocks = buildBlocks(events, sessionId, exchangeSeq);
  return blocks;
}

/** Parse SSE text into structured events. */
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
        // Skip unparseable lines
      }
      event = '';
      data = '';
    }
  }

  return events;
}

/** Build AgentBlock list from parsed SSE events. */
function buildBlocks(
  events: SseEvent[],
  sessionId: string,
  exchangeSeq: number,
): AgentBlock[] {
  const blocks: AgentBlock[] = [];
  let seq = 0;

  // Per-index accumulators for content blocks.
  const contentBlocks = new Map<number, ContentBlock>();

  function ensureBlock(idx: number, type: string): ContentBlock {
    let b = contentBlocks.get(idx);
    if (!b) {
      b = { index: idx, type, text: '', toolInput: '', thinking: '' };
      contentBlocks.set(idx, b);
    }
    return b;
  }

  for (const evt of events) {
    const d = evt.data as Record<string, unknown> | undefined;
    if (!d) continue;
    const type = d.type as string | undefined;

    switch (type) {
      case 'content_block_start': {
        const idx = d.index as number;
        const cb = d.content_block as Record<string, unknown> | undefined;
        if (!cb) continue;
        const cbType = cb.type as string;
        const blk = ensureBlock(idx, cbType);
        if (cbType === 'tool_use') {
          blk.name = cb.name as string;
          blk.id = cb.id as string;
          blk.toolInput = JSON.stringify(cb.input ?? {});
        }
        if (cbType === 'thinking') {
          blk.thinking = (cb.thinking as string) ?? '';
        }
        break;
      }

      case 'content_block_delta': {
        const idx = d.index as number;
        const delta = d.delta as Record<string, string> | undefined;
        if (!delta) continue;
        const deltaType = delta.type;
        const blk = ensureBlock(idx, deltaType === 'input_json_delta' ? 'tool_use' : deltaType === 'text_delta' ? 'text' : deltaType === 'thinking_delta' ? 'thinking' : 'text');

        if (deltaType === 'text_delta') {
          blk.text += delta.text ?? '';
        } else if (deltaType === 'input_json_delta') {
          blk.toolInput += delta.partial_json ?? '';
        } else if (deltaType === 'thinking_delta') {
          blk.thinking += delta.thinking ?? '';
        }
        break;
      }

      case 'content_block_stop': {
        const idx = d.index as number;
        const blk = contentBlocks.get(idx);
        if (!blk) continue;

        if (blk.type === 'text') {
          if (blk.text) {
            blocks.push({
              id: `http:${sessionId}:${exchangeSeq}:text:${seq++}`,
              kind: 'text',
              text: blk.text,
              seq: exchangeSeq * 1000 + seq,
            });
          }
        } else if (blk.type === 'tool_use') {
          blocks.push({
            id: `http:${sessionId}:${exchangeSeq}:action:${seq++}`,
            kind: 'action',
            text: '',
            code: safeParseJsonField(blk.toolInput),
            label: toolLabel(blk.name ?? 'tool', blk.toolInput),
            status: 'done', // From HTTP perspective, CLI has already executed
            seq: exchangeSeq * 1000 + seq,
          });
        } else if (blk.type === 'thinking') {
          if (blk.thinking) {
            blocks.push({
              id: `http:${sessionId}:${exchangeSeq}:thought:${seq++}`,
              kind: 'thought',
              text: blk.thinking,
              seq: exchangeSeq * 1000 + seq,
            });
          }
        }
        contentBlocks.delete(idx);
        break;
      }

      case 'message_stop':
        // Turn complete — residual blocks get flushed
        for (const [, blk] of contentBlocks) {
          if (blk.type === 'text' && blk.text) {
            blocks.push({
              id: `http:${sessionId}:${exchangeSeq}:text:${seq++}`,
              kind: 'text',
              text: blk.text,
              seq: exchangeSeq * 1000 + seq,
            });
          } else if (blk.type === 'tool_use') {
            blocks.push({
              id: `http:${sessionId}:${exchangeSeq}:action:${seq++}`,
              kind: 'action',
              text: '',
              code: safeParseJsonField(blk.toolInput),
              label: toolLabel(blk.name ?? 'tool', blk.toolInput),
              status: 'done',
              seq: exchangeSeq * 1000 + seq,
            });
          } else if (blk.type === 'thinking' && blk.thinking) {
            blocks.push({
              id: `http:${sessionId}:${exchangeSeq}:thought:${seq++}`,
              kind: 'thought',
              text: blk.thinking,
              seq: exchangeSeq * 1000 + seq,
            });
          }
        }
        contentBlocks.clear();
        break;

      case 'error':
        blocks.push({
          id: `http:${sessionId}:${exchangeSeq}:error:${seq++}`,
          kind: 'error',
          text: (d.error as Record<string, string>)?.message ?? JSON.stringify(d),
          seq: exchangeSeq * 1000 + seq,
        });
        break;
    }
  }

  return blocks;
}

// ── Request body parsing (tool results) ────────────────────────────

/**
 * Extract tool_result content from the NEXT API request body.
 *
 * When the CLI executes a tool (bash, read, etc.), the output is sent back
 * to the API in the *next* request's `messages` array as `tool_result`
 * content blocks.  This function extracts those so the visual layer can
 * show tool execution results.
 */
export function parseRequestToolResults(
  body: Uint8Array,
  sessionId: string,
  exchangeSeq: number,
): AgentBlock[] {
  let json: unknown;
  try {
    json = JSON.parse(new TextDecoder().decode(body));
  } catch {
    return [];
  }

  const root = json as Record<string, unknown> | undefined;
  if (!root?.messages) return [];

  const messages = root.messages as Array<Record<string, unknown>>;
  const blocks: AgentBlock[] = [];
  let seq = 0;

  for (const msg of messages) {
    const role = msg.role as string;
    if (role !== 'user') continue;

    const content = msg.content;
    if (!Array.isArray(content)) continue;

    for (const block of content) {
      const b = block as Record<string, unknown>;
      if (b.type !== 'tool_result') continue;

      const text = extractToolResultContent(b);
      const isError = !!b.is_error;
      blocks.push({
        id: `http:${sessionId}:${exchangeSeq}:obs:${seq++}`,
        kind: isError ? 'error' : 'observation',
        text,
        status: isError ? undefined : 'done',
        seq: exchangeSeq * 1000 + seq,
      });
    }
  }

  return blocks;
}

/**
 * Parse a complete HTTP exchange (request + response) into AgentBlock[].
 *
 * The response SSE stream provides AI text, thinking, and tool_use declarations.
 * The request body provides tool_result content from the previous turn.
 */
export function parseHttpExchange(
  reqBody: Uint8Array,
  respBody: Uint8Array,
  sessionId: string,
  exchangeSeq: number,
): AgentBlock[] {
  const respBlocks = parseSseResponse(respBody, sessionId, exchangeSeq);
  const toolResults = parseRequestToolResults(reqBody, sessionId, exchangeSeq);

  // Tool results precede the response blocks in conversation order:
  // user message → [tool results from last turn] → [new AI response]
  return [...toolResults, ...respBlocks];
}

// ── Helpers ─────────────────────────────────────────────────────────

/** Build a human-readable tool label from name + input. */
function toolLabel(name: string, inputJson: string): string {
  let input: Record<string, unknown> | undefined;
  try {
    input = JSON.parse(inputJson);
  } catch {
    return name;
  }

  switch (name) {
    case 'Bash': {
      const cmd = (input?.command as string) ?? '';
      const words = cmd.split(/\s+/).slice(0, 6).join(' ');
      return `bash: ${words}`;
    }
    case 'Edit':
    case 'Write': {
      const path = (input?.file_path as string) ?? (input?.path as string) ?? '';
      return `${name} ${path}`;
    }
    case 'Read': {
      const path = (input?.file_path as string) ?? (input?.path as string) ?? '';
      return `Read ${path}`;
    }
    default:
      return name;
  }
}

/** Extract text content from a tool_result block. */
function extractToolResultContent(block: Record<string, unknown>): string {
  const content = block.content;
  if (typeof content === 'string') return content;
  if (Array.isArray(content)) {
    return (content as Array<Record<string, unknown>>)
      .filter((c) => c.type === 'text')
      .map((c) => c.text ?? '')
      .join('\n');
  }
  return JSON.stringify(content ?? '');
}

/** Try to pretty-print JSON input; fall back to raw string. */
function safeParseJsonField(raw: string): string | undefined {
  if (!raw) return undefined;
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}
