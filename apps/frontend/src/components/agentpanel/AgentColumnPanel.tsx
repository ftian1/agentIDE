/**
 * AgentColumnPanel — the right-hand column for an agent session.
 *
 * Two tabs, toggled top-right:
 *   - 视觉交互层 (Structured): the parsed conversation view (AgentPanel). The
 *     parsing pipeline (agent_parse → approval cards, editor diff) runs in the
 *     backend regardless of which tab is visible — this tab just renders it.
 *   - 原生终端 (Raw): a full Xterm.js mirror of the agent CLI's colored TUI,
 *     for when the parsed layer misbehaves and you need the raw logs.
 *
 * Both tabs stay mounted; the inactive one is hidden (not unmounted) so the
 * Xterm scrollback and structured history survive tab switches.
 */
import { useState } from 'react';
import { AgentPanel } from './AgentPanel';
import { TerminalInstance } from '../terminal/TerminalInstance';
import { ActiveFilesMenu } from '../context/ActiveFilesMenu';
import { useTerminalApi } from '../../hooks/useTerminalApi';
import { useSessionStore } from '../../stores/sessionStore';

type AgentTab = 'structured' | 'raw';

export function AgentColumnPanel() {
  const [tab, setTab] = useState<AgentTab>('structured');
  const activeSessionId = useSessionStore((s) => s.activeSessionId);
  const api = useTerminalApi();

  return (
    <div className="flex flex-col h-full">
      {/* Header: tab toggle + active-files menu */}
      <div className="flex items-center justify-between px-2 h-8 flex-shrink-0 bg-bg-secondary border-b border-border">
        <div className="flex items-center gap-0.5">
          <TabButton active={tab === 'structured'} onClick={() => setTab('structured')}>
            视觉交互层
          </TabButton>
          <TabButton active={tab === 'raw'} onClick={() => setTab('raw')}>
            原生终端
          </TabButton>
        </div>
        <ActiveFilesMenu sessionId={activeSessionId} />
      </div>

      {/* Body — both tabs mounted, inactive one hidden to preserve state. */}
      <div className="flex-1 relative overflow-hidden">
        <div className={`absolute inset-0 ${tab === 'structured' ? '' : 'hidden'}`}>
          <AgentPanel sessionId={activeSessionId} />
        </div>
        <div className={`absolute inset-0 bg-bg-primary ${tab === 'raw' ? '' : 'hidden'}`}>
          {activeSessionId ? (
            <TerminalInstance key={activeSessionId} sessionId={activeSessionId} api={api} active={tab === 'raw'} />
          ) : (
            <div className="flex items-center justify-center h-full">
              <p className="text-xs text-text-secondary italic">无活动会话</p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function TabButton({
  active, onClick, children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className={`px-2.5 py-1 text-xs rounded transition-colors ${
        active ? 'text-text-primary bg-bg-tertiary' : 'text-text-secondary hover:text-text-primary'
      }`}
    >
      {children}
    </button>
  );
}
