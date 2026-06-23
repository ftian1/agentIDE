/**
 * Empty-state scaffolds for bottom-panel tabs without a live data source yet.
 * TODO: wire to backend feeds (MCP log stream, file sync status, port forwards).
 */
function EmptyPanel({ label }: { label: string }) {
  return (
    <div className="flex items-center justify-center h-full">
      <p className="text-xs text-text-secondary italic">{label}</p>
    </div>
  );
}

export function McpLogs() {
  return <EmptyPanel label="暂无 MCP / 插件日志" />;
}

export function FileSyncPanel() {
  return <EmptyPanel label="文件同步空闲 — 无进行中的同步" />;
}

export function PortsPanel() {
  return <EmptyPanel label="暂无转发端口" />;
}
