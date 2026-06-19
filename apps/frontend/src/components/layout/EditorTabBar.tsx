/**
 * EditorTabBar — top tab bar for open editors / code change files.
 */
import { useLayoutStore } from '../../stores/layoutStore';

export function EditorTabBar() {
  const tabs = useLayoutStore((s) => s.editorTabs);
  const activeId = useLayoutStore((s) => s.activeEditorTabId);
  const setActive = useLayoutStore((s) => s.setActiveEditorTab);
  const remove = useLayoutStore((s) => s.removeEditorTab);

  if (tabs.length === 0) return null;

  return (
    <div className="flex items-center bg-bg-secondary border-b border-border h-9 flex-shrink-0 px-1 gap-0.5 overflow-x-auto">
      {tabs.map((tab) => (
        <div
          key={tab.id}
          onClick={() => setActive(tab.id)}
          className={`
            group flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-t cursor-pointer
            transition-colors flex-shrink-0 max-w-[200px]
            ${tab.id === activeId
              ? 'bg-bg-primary text-text-primary border-t border-x border-border'
              : 'text-text-secondary hover:bg-bg-tertiary hover:text-text-primary border-t border-x border-transparent'
            }
          `}
        >
          <span className="truncate flex-1">{tab.label}</span>
          <button
            onClick={(e) => {
              e.stopPropagation();
              remove(tab.id);
            }}
            className="opacity-0 group-hover:opacity-100 hover:text-red-400 text-text-secondary
                       transition-opacity flex-shrink-0"
          >
            ✕
          </button>
        </div>
      ))}
    </div>
  );
}
