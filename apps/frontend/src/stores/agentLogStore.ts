/**
 * Agent Log Store — flat log feed for the bottom "Agent Stdout" panel.
 *
 * Aggregates session:event + agent:event into human-readable log lines.
 * Listeners are set up via initAgentLogListeners() at app startup.
 */
import { create } from 'zustand';

export interface LogLine {
  id: number;
  channel: 'agent' | 'session';
  text: string;
}

interface AgentLogStore {
  lines: LogLine[];
  _push: (channel: LogLine['channel'], text: string) => void;
  clear: () => void;
}

const MAX_LINES = 1000;
let lineSeq = 0;

export const useAgentLogStore = create<AgentLogStore>((set) => ({
  lines: [],
  _push: (channel, text) =>
    set((s) => {
      const next = [...s.lines, { id: ++lineSeq, channel, text }];
      if (next.length > MAX_LINES) next.splice(0, next.length - MAX_LINES);
      return { lines: next };
    }),
  clear: () => set({ lines: [] }),
}));

export function initAgentLogListeners() {
  import('@tauri-apps/api/event')
    .then(({ listen }) => {
      listen<{ session_id: string; kind: string; text: string; label?: string }>(
        'agent:event',
        (event) => {
          const p = event.payload;
          const head = p.label ? `${p.kind} ${p.label}` : p.kind;
          useAgentLogStore
            .getState()
            ._push('agent', `[agent] • ${head}${p.text ? ` ${p.text}` : ''}`);
        }
      );

      listen<{ session_id: string; event_type: string }>('session:event', (event) => {
        const p = event.payload;
        useAgentLogStore
          .getState()
          ._push('session', `[session ${p.session_id.slice(0, 8)}] ${p.event_type}`);
      });
    })
    .catch(() => {
      // Tauri API not available (browser dev mode).
    });
}
