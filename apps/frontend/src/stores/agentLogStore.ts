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

      listen<{ session_id: string; event_type: string; data?: Record<string, string> }>(
        'session:event',
        (event) => {
          const p = event.payload;
          const sid = p.session_id.slice(0, 8);
          // Format launch command with details for debugging.
          if (p.event_type === 'shellCommand' && p.data?.command) {
            useAgentLogStore.getState()._push('session', `[session ${sid}] ▶ LAUNCH: ${p.data.command}`);
            if (p.data.cwd) {
              useAgentLogStore.getState()._push('session', `[session ${sid}]   cwd: ${p.data.cwd}`);
            }
            for (const [k, v] of Object.entries(p.data)) {
              if (k !== 'command' && k !== 'cwd') {
                useAgentLogStore.getState()._push('session', `[session ${sid}]   env: ${k}=${v}`);
              }
            }
          } else {
            useAgentLogStore.getState()._push('session', `[session ${sid}] ${p.event_type}`);
          }
        }
      );
    })
    .catch(() => {
      // Tauri API not available (browser dev mode).
    });
}
