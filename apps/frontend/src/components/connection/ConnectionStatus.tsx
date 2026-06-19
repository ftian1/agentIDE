/** Small indicator showing current connection status. */
export function ConnectionStatus() {
  return (
    <div className="flex items-center gap-2 text-xs text-text-secondary">
      <span className="w-2 h-2 rounded-full bg-gray-500" />
      <span>Disconnected</span>
    </div>
  );
}
