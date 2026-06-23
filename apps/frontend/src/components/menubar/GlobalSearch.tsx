/**
 * GlobalSearch — centered top-bar search input.
 */
import { useState } from 'react';
import { Search } from 'lucide-react';

export function GlobalSearch() {
  const [value, setValue] = useState('');

  return (
    <div className="relative w-full max-w-[520px]">
      <Search
        size={14}
        strokeWidth={1.5}
        className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-secondary pointer-events-none"
      />
      <input
        type="text"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        placeholder="Search files, commands, or ask Agent..."
        // TODO: wire up command palette / file search / agent ask
        className="w-full bg-bg-tertiary text-text-primary text-sm pl-8 pr-3 py-1 rounded
                   border border-border focus:outline-none focus:border-accent
                   placeholder:text-text-secondary"
      />
    </div>
  );
}
