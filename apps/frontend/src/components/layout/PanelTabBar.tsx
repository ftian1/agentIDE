/**
 * PanelTabBar — tab bar for the bottom panel.
 */
import { useLayoutStore } from '../../stores/layoutStore';
import type { BottomPanelTab } from '../../stores/layoutStore';

const TABS: { id: BottomPanelTab; label: string }[] = [
  { id: 'agentStdout', label: 'Agent Stdout' },
  { id: 'mcpLogs', label: 'MCP / 插件日志' },
  { id: 'fileSync', label: '文件同步 Sync' },
  { id: 'problems', label: '问题' },
  { id: 'ports', label: '端口' },
];

export function PanelTabBar() {
  const active = useLayoutStore((s) => s.bottomPanelTab);
  const setTab = useLayoutStore((s) => s.setBottomPanelTab);

  return (
    <div className="flex items-center bg-bg-tertiary border-b border-border px-1 h-8 flex-shrink-0">
      {TABS.map((tab) => (
        <button
          key={tab.id}
          onClick={() => setTab(tab.id)}
          className={`
            relative px-3 py-1 text-xs transition-colors
            ${active === tab.id
              ? 'text-text-primary border-t-2 border-accent -mt-px bg-bg-secondary'
              : 'text-text-secondary hover:text-text-primary border-t-2 border-transparent'
            }
          `}
        >
          {tab.label}
        </button>
      ))}
    </div>
  );
}
