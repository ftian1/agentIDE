/**
 * Agent Store — Zustand store for the Claude Code agent conversation.
 *
 * Receives agent:event / agent:status events from the backend stream parser.
 * Listeners are set up via initAgentListeners() at app startup (codeChangeStore
 * pattern) to avoid side-effects in the store creator.
 */
import { create } from 'zustand';

export type AgentBlockKind = 'thought' | 'action' | 'observation';

export interface AgentBlock {
  id: string;
  kind: AgentBlockKind;
  text: string;
  code?: string;
  label?: string;
  status?: string;
  seq: number;
}

export interface AgentTurn {
  id: string;
  sessionId: string;
  role: 'user' | 'agent';
  text?: string;
  blocks: AgentBlock[];
  createdAt: string;
}

export interface AgentStore {
  turns: Record<string, AgentTurn[]>;
  statusLine: Record<string, string>;

  _addUserTurn: (sessionId: string, text: string) => void;
  _appendBlock: (sessionId: string, block: AgentBlock) => void;
  _setStatus: (sessionId: string, text: string) => void;
  clear: (sessionId: string) => void;

  sendMessage: (sessionId: string, text: string) => Promise<void>;
}

let turnSeq = 0;
const nextId = () => `t${++turnSeq}`;

export const useAgentStore = create<AgentStore>((set, get) => ({
  turns: {},
  statusLine: {},

  _addUserTurn: (sessionId, text) =>
    set((s) => {
      const list = s.turns[sessionId] ?? [];
      const turn: AgentTurn = {
        id: nextId(),
        sessionId,
        role: 'user',
        text,
        blocks: [],
        createdAt: new Date().toISOString(),
      };
      return { turns: { ...s.turns, [sessionId]: [...list, turn] } };
    }),

  _appendBlock: (sessionId, block) =>
    set((s) => {
      const list = s.turns[sessionId] ?? [];
      const last = list[list.length - 1];
      // Append to the trailing agent turn; otherwise open a new agent turn.
      if (last && last.role === 'agent') {
        const updated: AgentTurn = { ...last, blocks: [...last.blocks, block] };
        return {
          turns: { ...s.turns, [sessionId]: [...list.slice(0, -1), updated] },
        };
      }
      const turn: AgentTurn = {
        id: nextId(),
        sessionId,
        role: 'agent',
        blocks: [block],
        createdAt: new Date().toISOString(),
      };
      return { turns: { ...s.turns, [sessionId]: [...list, turn] } };
    }),

  _setStatus: (sessionId, text) =>
    set((s) => ({ statusLine: { ...s.statusLine, [sessionId]: text } })),

  clear: (sessionId) =>
    set((s) => {
      const turns = { ...s.turns };
      const statusLine = { ...s.statusLine };
      delete turns[sessionId];
      delete statusLine[sessionId];
      return { turns, statusLine };
    }),

  async sendMessage(sessionId, text) {
    get()._addUserTurn(sessionId, text);
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('send_agent_message', { sessionId, text });
    } catch {
      // TODO: surface send failure to the user (command may not exist yet).
    }
  },
}));

/**
 * Derive a short agent-activity string for the status bar, e.g.
 * "[Executing Tool: Bash]" or "[Idle]", from the latest blocks of a session.
 *
 * Heuristic: if the trailing block is an Action (tool_use) we report the tool;
 * if it's an Observation/Thought the agent is thinking; otherwise idle.
 */
export function deriveAgentActivity(state: AgentStore, sessionId: string | null): string {
  if (!sessionId) return 'Idle';
  const turns = state.turns[sessionId];
  if (!turns || turns.length === 0) return 'Idle';
  const last = turns[turns.length - 1];
  if (!last || last.blocks.length === 0) {
    return last?.role === 'user' ? 'Thinking…' : 'Idle';
  }
  const block = last.blocks[last.blocks.length - 1];
  switch (block.kind) {
    case 'action': {
      // label is like "bash: ls -la" or "Edit /path" — take the leading verb.
      const tool = (block.label ?? '').split(/[:\s]/)[0] || 'Tool';
      const cap = tool.charAt(0).toUpperCase() + tool.slice(1);
      return `Executing Tool: ${cap}`;
    }
    case 'observation':
      return 'Processing result…';
    case 'thought':
      return 'Thinking…';
    default:
      return 'Idle';
  }
}

/**
 * Initialize agent event listeners. Call once at app startup.
 */
export function initAgentListeners() {
  import('@tauri-apps/api/event')
    .then(({ listen }) => {
      listen<{
        session_id: string;
        kind: AgentBlockKind;
        text: string;
        code?: string;
        label?: string;
        status?: string;
        seq: number;
      }>('agent:event', (event) => {
        const p = event.payload;
        useAgentStore.getState()._appendBlock(p.session_id, {
          id: `b${p.seq}-${p.kind}`,
          kind: p.kind,
          text: p.text,
          code: p.code,
          label: p.label,
          status: p.status,
          seq: p.seq,
        });

        // Action blocks carry tool-use labels like "Read /a/b.ts" — feed file
        // touches into the context-file store powering "Show Active Files".
        if (p.kind === 'action') {
          import('./contextFileStore').then(({ useContextFileStore, parseActionLabel }) => {
            const parsed = parseActionLabel(p.label);
            if (parsed) {
              useContextFileStore.getState()._touch(p.session_id, parsed.path, parsed.origin, p.seq);
            }
          });
        }
      });

      listen<{ session_id: string; text: string }>('agent:status', (event) => {
        const p = event.payload;
        useAgentStore.getState()._setStatus(p.session_id, p.text);
      });
    })
    .catch(() => {
      // Tauri API not available (browser dev mode).
    });
}
