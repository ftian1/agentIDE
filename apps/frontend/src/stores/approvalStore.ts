/**
 * Approval Store — Zustand store for the agent approval flow queue.
 *
 * Receives approval:request events from the backend (agent permission prompts).
 * Listeners are set up via initApprovalListeners() at app startup, mirroring
 * the codeChangeStore pattern, to avoid side-effects in the store creator.
 */
import { create } from 'zustand';

export type ApprovalDecision = 'allow' | 'allowAll' | 'reject';
export type ApprovalStatus = 'pending' | 'resolved';

export interface ApprovalRequest {
  id: string;
  sessionId: string;
  title: string;
  scope: string;
  command: string;
  cwd?: string;
  status: ApprovalStatus;
  decision?: ApprovalDecision;
  createdAt: string;
}

interface ApprovalStore {
  requests: Record<string, ApprovalRequest>;
  _addRequest: (req: ApprovalRequest) => void;
  _resolve: (id: string, decision: ApprovalDecision) => void;
  respond: (id: string, decision: ApprovalDecision) => Promise<void>;
}

export const useApprovalStore = create<ApprovalStore>((set, get) => ({
  requests: {},

  _addRequest: (req) =>
    set((s) => ({ requests: { ...s.requests, [req.id]: req } })),

  _resolve: (id, decision) =>
    set((s) => {
      const cur = s.requests[id];
      if (!cur) return s;
      return {
        requests: {
          ...s.requests,
          [id]: { ...cur, status: 'resolved', decision },
        },
      };
    }),

  async respond(id, decision) {
    const req = get().requests[id];
    if (!req) return;
    // Optimistic: resolve locally first.
    get()._resolve(id, decision);
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('respond_approval', {
        requestId: id,
        sessionId: req.sessionId,
        decision,
      });
    } catch {
      // TODO: surface failure / re-open the request if the backend rejects.
    }
  },
}));

/**
 * Initialize approval event listeners. Call once at app startup.
 */
export function initApprovalListeners() {
  import('@tauri-apps/api/event')
    .then(({ listen }) => {
      listen<{
        session_id: string;
        request_id: string;
        title: string;
        command: string;
        cwd?: string;
        scope?: string;
      }>('approval:request', (event) => {
        const p = event.payload;
        useApprovalStore.getState()._addRequest({
          id: p.request_id,
          sessionId: p.session_id,
          title: p.title,
          scope: p.scope ?? '',
          command: p.command,
          cwd: p.cwd,
          status: 'pending',
          createdAt: new Date().toISOString(),
        });
      });
    })
    .catch(() => {
      // Tauri API not available (browser dev mode).
    });
}
