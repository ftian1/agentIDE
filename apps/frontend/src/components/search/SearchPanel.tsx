/** Search panel placeholder for the Search activity. */
export function SearchPanel() {
  return (
    <div className="flex flex-col h-full">
      <div className="px-3 py-3 border-b border-border">
        <span className="text-xs font-semibold text-text-secondary uppercase tracking-wider">
          Search
        </span>
      </div>
      <div className="p-3">
        <input
          type="text"
          placeholder="Search across sessions..."
          className="w-full bg-bg-tertiary text-text-primary text-xs px-2 py-1.5 rounded border border-border
                     focus:outline-none focus:border-accent placeholder:text-text-secondary"
        />
      </div>
      <div className="flex-1 flex items-center justify-center">
        <p className="text-xs text-text-secondary italic">Search results will appear here.</p>
      </div>
    </div>
  );
}
