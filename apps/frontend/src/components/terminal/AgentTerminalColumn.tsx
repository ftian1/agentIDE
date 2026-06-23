/**
 * AgentTerminalColumn — the right-hand column hosting the agent CLI terminal.
 *
 * Layout: a header bar (with the "Show Active Files" context menu) above the
 * Xterm.js terminal that renders the remote agent CLI's TUI. This is the right
 * window in the three-region IDE layout (editor · terminal · status bar).
 */
import { TerminalPane } from './TerminalPane';
import { ActiveFilesMenu } from '../context/ActiveFilesMenu';
import { useSessionStore } from '../../stores/sessionStore';

export function AgentTerminalColumn() {
  const activeSessionId = useSessionStore((s) => s.activeSessionId);

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between px-2 h-8 flex-shrink-0 bg-bg-secondary border-b border-border">
        <span className="text-xs font-semibold text-text-secondary uppercase tracking-wider pl-1">
          Terminal
        </span>
        <ActiveFilesMenu sessionId={activeSessionId} />
      </div>
      <div className="flex-1 overflow-hidden">
        <TerminalPane />
      </div>
    </div>
  );
}
