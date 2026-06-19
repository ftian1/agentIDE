interface Props {
  onToggleDetail: () => void;
}

/** Bottom status bar: connection state, tool status, session count. */
export function StatusPanel({ onToggleDetail }: Props) {
  return (
    <div className="flex items-center gap-4 w-full text-xs text-text-secondary">
      {/* Connection status */}
      <div className="flex items-center gap-1.5">
        <span className="w-1.5 h-1.5 rounded-full bg-gray-500" />
        <span>No connection</span>
      </div>

      {/* Spacer */}
      <div className="flex-1" />

      {/* Right-side controls */}
      <button
        onClick={onToggleDetail}
        className="hover:text-text-primary transition-colors"
      >
        Toggle Detail
      </button>
    </div>
  );
}
