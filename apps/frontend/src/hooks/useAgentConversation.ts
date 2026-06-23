/**
 * useAgentConversation — headless view-model for the agent panel.
 */
import { useAgentStore } from '../stores/agentStore';
import type { AgentTurn } from '../stores/agentStore';

export interface AgentConversationView {
  turns: AgentTurn[];
  statusLine: string;
  sendMessage: (text: string) => void;
}

const EMPTY: AgentTurn[] = [];

export function useAgentConversation(sessionId: string | null): AgentConversationView {
  const turns = useAgentStore((s) => (sessionId ? s.turns[sessionId] ?? EMPTY : EMPTY));
  const statusLine = useAgentStore((s) => (sessionId ? s.statusLine[sessionId] ?? '' : ''));
  const send = useAgentStore((s) => s.sendMessage);

  return {
    turns,
    statusLine,
    sendMessage: (text: string) => {
      if (sessionId) send(sessionId, text);
    },
  };
}
