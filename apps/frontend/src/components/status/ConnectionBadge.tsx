/**
 * ConnectionBadge — SSH connection + latency indicator for the top bar.
 */
import { useConnectionStore } from '../../stores/connectionStore';

export function ConnectionBadge() {
  const connected = useConnectionStore((s) =>
    Object.values(s.connections).some((c) => c.status === 'connected')
  );

  return (
    <div className="flex items-center gap-1.5 px-2 text-xs text-text-secondary whitespace-nowrap">
      <span
        className={`w-1.5 h-1.5 rounded-full ${connected ? 'bg-green-400' : 'bg-gray-500'}`}
      />
      <span>{connected ? 'SSH 已连接' : '未连接'}</span>
      {connected && <span className="opacity-60">· 12ms{/* TODO: real latency */}</span>}
    </div>
  );
}
