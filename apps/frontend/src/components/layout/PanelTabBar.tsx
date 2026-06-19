/**
 * PanelTabBar — tab bar for the bottom panel.
 */
import { useLayoutStore } from '../../stores/layoutStore';
import type { BottomPanelTab } from '../../stores/layoutStore';
import { useCodeChangeStore } from '../../stores/codeChangeStore';

const TABS: { id: BottomPanelTab; label: string }[] = [
  { id: 'terminal', label: 'Terminal' },
  { id: 'problems', label: 'Problems' },
  { id: 'output', label: 'Output' },
  { id: 'codeChanges', label: 'Code Changes' },
];

export function PanelTabBar() {
  const active = useLayoutStore((s) => s.bottomPanelTab);
  const setTab = useLayoutStore((s) => s.setBottomPanelTab);
  const changeCount = useCodeChangeStore((s) =>
    Object.values(s.changeSets).reduce(
      (sum, cs) =>
        sum +
        Object.values(cs.files).filter((f) => f.status === 'pending').length,
      0
    )
  );

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
          {tab.id === 'codeChanges' && changeCount > 0 && (
            <span className="ml-1.5 px-1 py-0.5 text-[10px] rounded bg-accent/20 text-accent">
              {changeCount}
            </span>
          )}
        </button>
      ))}
    </div>
  );
}
