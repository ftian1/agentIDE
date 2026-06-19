/**
 * ActivityBar — 48px leftmost icon column.
 * Navigation hub: Explorer, Search, Source Control, Settings.
 */
import { FolderTree, Search, GitBranch, Settings } from 'lucide-react';
import { useLayoutStore } from '../../stores/layoutStore';
import type { ActivityId } from '../../stores/layoutStore';
import { useCodeChangeStore } from '../../stores/codeChangeStore';

interface ActivityItem {
  id: ActivityId;
  icon: typeof FolderTree;
  label: string;
  badge?: number;
}

export function ActivityBar() {
  const active = useLayoutStore((s) => s.activeActivity);
  const setActive = useLayoutStore((s) => s.setActiveActivity);
  const pendingChanges = useCodeChangeStore((s) =>
    Object.values(s.changeSets).filter((cs) => cs.status === 'pending' || cs.status === 'complete')
      .reduce((sum, cs) => sum + Object.keys(cs.files).length, 0)
  );

  const items: ActivityItem[] = [
    { id: 'explorer', icon: FolderTree, label: 'Explorer' },
    { id: 'search', icon: Search, label: 'Search' },
    { id: 'sourceControl', icon: GitBranch, label: 'Source Control', badge: pendingChanges || undefined },
    { id: 'settings', icon: Settings, label: 'Settings' },
  ];

  return (
    <div className="w-12 flex-shrink-0 bg-bg-secondary border-r border-border flex flex-col items-center py-2 gap-1">
      {items.map((item) => {
        const isActive = active === item.id;
        return (
          <button
            key={item.id}
            onClick={() => setActive(item.id)}
            className={`
              relative w-10 h-10 flex items-center justify-center rounded-md
              transition-colors duration-100
              ${isActive
                ? 'text-accent bg-accent/10'
                : 'text-text-secondary hover:text-text-primary hover:bg-bg-tertiary'
              }
            `}
            title={item.label}
          >
            {/* Left accent border when active */}
            {isActive && (
              <span className="absolute left-0 top-1 bottom-1 w-0.5 bg-accent rounded-r" />
            )}
            <item.icon size={20} strokeWidth={1.5} />
            {/* Badge */}
            {item.badge && item.badge > 0 && (
              <span className="absolute top-0.5 right-0.5 min-w-[14px] h-[14px] flex items-center justify-center
                               bg-accent text-[9px] font-bold text-white rounded-full px-0.5">
                {item.badge > 99 ? '99+' : item.badge}
              </span>
            )}
          </button>
        );
      })}
    </div>
  );
}
