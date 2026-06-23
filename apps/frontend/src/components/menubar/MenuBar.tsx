/**
 * MenuBar — top chrome: app title + 5 menus + centered global search.
 *
 * The right-side connection badge is injected by the caller (App.tsx) so this
 * stays decoupled from the connection store.
 */
import type { ReactNode } from 'react';
import { ExternalLink } from 'lucide-react';
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
    <div className="h-9 flex items-center gap-1 px-2 bg-bg-secondary border-b border-border flex-shrink-0">
      <span className="text-xs font-semibold text-text-primary px-1 whitespace-nowrap">
        Remote AI IDE
      </span>
      {menus.map((m) => (
        <MenuDropdown key={m.label} label={m.label} items={m.items} />
      ))}

      <div className="flex-1 flex justify-center px-4">
        <GlobalSearch />
      </div>

      <div className="flex items-center">{rightSlot}</div>
    </div>
  );
}
