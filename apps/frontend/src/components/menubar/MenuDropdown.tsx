/**
 * MenuDropdown — reusable top-bar menu button with a click-to-open dropdown.
 */
import { useEffect, useRef, useState } from 'react';
import type { LucideIcon } from 'lucide-react';

export interface MenuItemSpec {
  label: string;
  shortcut?: string;
  icon?: LucideIcon;
  danger?: boolean;
  divider?: boolean;
  onClick?: () => void;
}

interface Props {
  label: string;
  items: MenuItemSpec[];
}

export function MenuDropdown({ label, items }: Props) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', onDown);
    return () => document.removeEventListener('mousedown', onDown);
  }, [open]);

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        className={`px-2 py-1 text-xs rounded transition-colors ${
          open
            ? 'text-text-primary bg-bg-tertiary'
            : 'text-text-secondary hover:text-text-primary hover:bg-bg-tertiary'
        }`}
      >
        {label}
      </button>

      {open && (
        <div
          className="absolute left-0 top-full mt-1 min-w-[240px] z-50
                     bg-bg-secondary border border-border rounded-md shadow-2xl py-1"
        >
          {items.map((item, i) =>
            item.divider ? (
              <div key={`d${i}`} className="h-px bg-border my-1" />
            ) : (
              <button
                key={item.label}
                onClick={() => {
                  setOpen(false);
                  item.onClick?.();
                }}
                className={`w-full flex items-center justify-between gap-6 px-3 py-1.5 text-xs text-left
                           transition-colors hover:bg-bg-tertiary ${
                             item.danger
                               ? 'text-red-400 hover:text-red-300'
                               : 'text-text-secondary hover:text-text-primary'
                           }`}
              >
                <span className="flex items-center gap-2">
                  {item.label}
                  {item.icon && <item.icon size={12} strokeWidth={1.5} />}
                </span>
                {item.shortcut && (
                  <span className="text-[10px] text-text-secondary opacity-70 font-mono">
                    {item.shortcut}
                  </span>
                )}
              </button>
            )
          )}
        </div>
      )}
    </div>
  );
}
