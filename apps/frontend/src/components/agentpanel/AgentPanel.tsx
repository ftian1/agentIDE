/**
 * AgentPanel — right-side "Claude Code" agent conversation panel.
 */
import { useAgentConversation } from '../../hooks/useAgentConversation';
import { AgentTurn } from './AgentTurn';
import { AgentInput } from './AgentInput';

interface Props {
  sessionId: string | null;
}

export function AgentPanel({ sessionId }: Props) {
  const { turns, statusLine, sendMessage } = useAgentConversation(sessionId);

  if (!sessionId) {
    return (
      <div className="flex flex-col h-full items-center justify-center">
        <p className="text-xs text-text-secondary italic">无活动会话</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-border flex-shrink-0">
        <div className="flex items-center gap-2">
          <span className="text-sm font-semibold text-text-primary">Claude Code</span>
          <span className="w-1.5 h-1.5 rounded-full bg-green-400" />
          <span className="text-xs text-text-secondary">Auto · Max</span>
        </div>
        <div className="flex items-center gap-1">
          <button className="px-2 py-0.5 text-xs text-text-secondary hover:text-text-primary rounded transition-colors">
            继续
          </button>
          <button className="px-2 py-0.5 text-xs text-text-secondary hover:text-text-primary rounded transition-colors">
            后台
          </button>
        </div>
      </div>

      {/* Conversation */}
      <div className="flex-1 overflow-y-auto px-3 py-2 space-y-3">
        {turns.length === 0 ? (
          <p className="text-xs text-text-secondary italic">开始与 Agent 对话...</p>
        ) : (
          turns.map((t) => <AgentTurn key={t.id} turn={t} />)
        )}
      </div>

      {/* Status line */}
      {statusLine && (
        <div className="px-3 py-1 text-xs text-text-secondary border-t border-border bg-bg-secondary">
          {statusLine}
        </div>
      )}

      {/* Input */}
      <AgentInput onSend={sendMessage} />
    </div>
  );
}
