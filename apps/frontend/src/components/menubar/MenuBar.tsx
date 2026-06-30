/**
 * MenuBar — top chrome: app title + 5 menus + centered global search.
 * Also serves as the custom title bar (frameless window) with min/max/close.
 *
 * The drag region covers only the logo + menus (left portion); the search
 * input and window controls are outside it so clicks always reach them.
 */

import { type ReactNode, useRef, useEffect } from 'react';
import { ExternalLink, Minus, Square, X } from 'lucide-react';
import { getCurrentWindow, type Window } from '@tauri-apps/api/window';
import { MenuDropdown } from './MenuDropdown';
import type { MenuItemSpec } from './MenuDropdown';
import { GlobalSearch } from './GlobalSearch';
import { useMenuCommands } from '../../hooks/useMenuCommands';

interface Props {
  /** Right-aligned slot (e.g. <ConnectionBadge/>). */
  rightSlot?: ReactNode;
}

export function MenuBar({ rightSlot }: Props) {
  const cmd = useMenuCommands();
  const winRef = useRef<Window | null>(null);

  // Init Tauri window handle once on mount.
  useEffect(() => {
    try {
      winRef.current = getCurrentWindow();
    } catch (e) {
      console.error('[MenuBar] getCurrentWindow failed:', e);
    }
  }, []);

  const handleMinimize = () => { winRef.current?.minimize(); };
  const handleToggleMaximize = () => { winRef.current?.toggleMaximize(); };
  const handleClose = () => { winRef.current?.close(); };
  const handleDoubleClick = () => { winRef.current?.toggleMaximize(); };

  const menus: { label: string; items: MenuItemSpec[] }[] = [
    {
      label: 'File / Remote',
      items: [
        { label: 'New / Open Remote Project...', icon: ExternalLink, onClick: cmd.openRemoteProject },
        { label: 'Remote Connection Manager...', icon: ExternalLink, onClick: cmd.openConnectionManager },
        { label: 'Sync Remote Filesystem', shortcut: 'Ctrl+R', onClick: cmd.syncRemoteFs },
      ],
    },
    {
      label: 'Agent Engine',
      items: [
        { label: 'Agent Backend Settings...', icon: ExternalLink, onClick: cmd.openAgentBackendSettings },
        { label: 'Model Route Override...', icon: ExternalLink, onClick: cmd.openModelRoute },
      ],
    },
    {
      label: 'Git & Review',
      items: [
        { label: 'Review Recent Agent Commits', onClick: cmd.reviewCommits },
        { divider: true, label: '' },
        { label: 'Undo Last Agent Session', danger: true, onClick: cmd.undoLastSession },
      ],
    },
    {
      label: 'View',
      items: [
        { label: 'Toggle File Explorer', shortcut: 'Ctrl+B', onClick: cmd.toggleFileExplorer },
        { label: 'Toggle Agent Panel', shortcut: 'Ctrl+J', onClick: cmd.toggleAgentPanel },
        { label: 'Toggle Terminal Dock', shortcut: 'Ctrl+Shift+P', onClick: cmd.toggleTerminalDock },
        { label: 'Split Editor Right', shortcut: 'Ctrl+K', onClick: cmd.splitEditorRight },
        { label: 'Zen Mode', shortcut: 'Ctrl+K Z', onClick: cmd.zenMode },
      ],
    },
    {
      label: 'Help',
      items: [
        { label: 'Documentation', onClick: cmd.openDocs },
        { label: 'Keyboard Shortcuts', shortcut: '⌘K ⌘S', onClick: cmd.openShortcuts },
        { divider: true, label: '' },
        { label: 'Release Notes', onClick: cmd.openReleaseNotes },
        { label: 'About Remote AI IDE', onClick: cmd.openAbout },
      ],
    },
  ];

  return (
    <div className="h-9 flex items-center gap-1 pl-2 pr-1 bg-bg-secondary border-b border-border flex-shrink-0 select-none">
      {/* Drag region: logo + menus (left portion) */}
      <div
        data-tauri-drag-region
        className="flex items-center gap-1 h-full"
        onDoubleClick={handleDoubleClick}
      >
        <span className="text-xs font-semibold text-text-primary px-1 whitespace-nowrap">
          Remote AI IDE
        </span>
        {menus.map((m) => (
          <MenuDropdown key={m.label} label={m.label} items={m.items} />
        ))}
      </div>

      {/* Search (non-drag so input always works) */}
      <div className="flex-1 flex justify-center px-4">
        <GlobalSearch />
      </div>

      {/* Right slot (connection badge etc.) */}
      <div className="flex items-center gap-1">{rightSlot}</div>

      {/* Window controls — outside drag region, always clickable */}
      <div className="flex items-center ml-1">
        <button
          onClick={handleMinimize}
          className="w-8 h-7 flex items-center justify-center text-text-secondary hover:text-text-primary hover:bg-bg-tertiary rounded transition-colors"
          aria-label="Minimize"
        >
          <Minus size={14} />
        </button>
        <button
          onClick={handleToggleMaximize}
          className="w-8 h-7 flex items-center justify-center text-text-secondary hover:text-text-primary hover:bg-bg-tertiary rounded transition-colors"
          aria-label="Maximize"
        >
          <Square size={12} />
        </button>
        <button
          onClick={handleClose}
          className="w-8 h-7 flex items-center justify-center text-text-secondary hover:text-white hover:bg-red-500 rounded transition-colors"
          aria-label="Close"
        >
          <X size={15} />
        </button>
      </div>
    </div>
  );
}
