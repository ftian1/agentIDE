/**
 * AgentPanel — right-side agent conversation panel.
 *
 * Subscribes to {@link useAgentConversation} for structured turns and
 * renders them with kind-dispatching visual components. Supports
 * clickable file paths that open remote files in the code editor.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { ArrowDown } from 'lucide-react';
import { useAgentConversation } from '../../hooks/useAgentConversation';
import { useSessionStore } from '../../stores/sessionStore';
import { useLayoutStore } from '../../stores/layoutStore';
import { AgentTurn } from './AgentTurn';
import { AgentInput } from './AgentInput';

interface Props {
  sessionId: string | null;
}

export function AgentPanel({ sessionId }: Props) {
  const { turns, statusLine, sendMessage } = useAgentConversation(sessionId);

  // Resolve the connectionId so we can open files in the editor.
  const connectionId = useSessionStore((s) =>
    sessionId ? s.sessions[sessionId]?.connectionId : null,
  );

  // ── Scroll-to-bottom management ──
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const sentinelRef = useRef<HTMLDivElement | null>(null);
  const autoScrollRef = useRef(true);
  const [showScrollBtn, setShowScrollBtn] = useState(false);

  // Track whether user has scrolled away from the bottom.
  useEffect(() => {
    const sentinel = sentinelRef.current;
    if (!sentinel) return;
    const obs = new IntersectionObserver(
      ([entry]) => {
        autoScrollRef.current = entry.isIntersecting;
        setShowScrollBtn(!entry.isIntersecting);
      },
      { root: scrollRef.current, threshold: 0 },
    );
    obs.observe(sentinel);
    return () => obs.disconnect();
  }, [sessionId]);

  // Auto-scroll when new turns arrive (if user hasn't scrolled up).
  useEffect(() => {
    if (autoScrollRef.current) {
      sentinelRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [turns.length]);

  // ── File opening ──
  const handleOpenFile = useCallback(
    (filePath: string) => {
      if (!connectionId) return;
      useLayoutStore.getState().addEditorTab({
        id: `agent:${sessionId}:${filePath}`,
        filePath,
        label: filePath.split('/').pop() || filePath,
        connectionId,
      });
    },
    [sessionId, connectionId],
  );

  // ── Empty state ──
  if (!sessionId) {
    return (
      <div className="flex flex-col h-full items-center justify-center">
        <p className="text-xs text-text-secondary italic">无活动会话</p>
      </div>
    );
  }

  const isStreaming =
    turns.length > 0 &&
    turns[turns.length - 1].role === 'agent';

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

      {/* Conversation area */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto px-3 py-2 relative">
        {turns.length === 0 ? (
          <p className="text-xs text-text-secondary italic text-center mt-4">
            开始与 Agent 对话…
          </p>
        ) : (
          turns.map((turn, i) => (
            <AgentTurn
              key={turn.id}
              turn={turn}
              isStreaming={isStreaming && i === turns.length - 1}
              onOpenFile={handleOpenFile}
            />
          ))
        )}

        {/* Sentinel for IntersectionObserver (bottom of scroll area) */}
        <div ref={sentinelRef} className="h-px" />

        {/* Floating "scroll to bottom" button */}
        {showScrollBtn && (
          <button
            onClick={() => sentinelRef.current?.scrollIntoView({ behavior: 'smooth' })}
            className="absolute bottom-2 right-3 p-1.5 rounded-full bg-bg-tertiary border border-border
                       text-text-secondary hover:text-text-primary shadow-md transition-all z-10"
            aria-label="滚动到底部"
          >
            <ArrowDown size={14} strokeWidth={1.5} />
          </button>
        )}
      </div>

      {/* Status line */}
      {statusLine && (
        <div className="px-3 py-1 text-xs text-text-secondary border-t border-border bg-bg-secondary flex-shrink-0">
          {statusLine}
        </div>
      )}

      {/* Input */}
      <AgentInput onSend={sendMessage} />
    </div>
  );
}
