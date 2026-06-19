/**
 * AgentStatus — displays detected CLI tools with version/auth badges.
 */
import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface ToolInfo {
  tool: string;
  installed: boolean;
  version?: string;
  auth_ok?: boolean;
}

const KNOWN_TOOLS = ['claude', 'copilot', 'node', 'python3'];

export function AgentStatus() {
  const [tools, setTools] = useState<Record<string, ToolInfo>>({});
  const [probing, setProbing] = useState(false);
  const [installing, setInstalling] = useState<string | null>(null);

  const probeAll = useCallback(async () => {
    setProbing(true);
    for (const tool of KNOWN_TOOLS) {
      try {
        const result = await invoke<{
          tool: string; installed: boolean; version?: string;
          path?: string; auth_ok?: boolean;
        }>('probe_tool', { connectionId: '', tool });
        setTools((prev) => ({ ...prev, [tool]: result }));
      } catch {
        setTools((prev) => ({ ...prev, [tool]: { tool, installed: false } }));
      }
    }
    setProbing(false);
  }, []);

  const install = useCallback(async (tool: string) => {
    setInstalling(tool);
    try {
      await invoke('install_tool', { connectionId: '', tool, version: null });
      // Re-probe after install
      const result = await invoke<ToolInfo>('probe_tool', { connectionId: '', tool });
      setTools((prev) => ({ ...prev, [tool]: result }));
    } catch (e) {
      console.error('Install failed:', e);
    }
    setInstalling(null);
  }, []);

  return (
    <div className="p-3 space-y-2">
      <div className="flex items-center justify-between">
        <span className="text-xs font-semibold text-text-secondary uppercase tracking-wider">
          Agent Tools
        </span>
        <button
          onClick={probeAll}
          disabled={probing}
          className="text-xs text-accent hover:text-blue-300 disabled:opacity-50"
        >
          {probing ? 'Scanning...' : 'Scan'}
        </button>
      </div>

      {KNOWN_TOOLS.map((tool) => {
        const info = tools[tool];
        return (
          <div
            key={tool}
            className="flex items-center justify-between px-2 py-1 rounded bg-bg-tertiary text-xs"
          >
            <div className="flex items-center gap-2">
              <span
                className={`w-2 h-2 rounded-full ${
                  info?.installed ? 'bg-green-400' : 'bg-gray-500'
                }`}
              />
              <span className="text-text-primary font-mono">{tool}</span>
              {info?.version && (
                <span className="text-text-secondary">{info.version}</span>
              )}
              {info?.auth_ok === true && (
                <span className="text-green-400 text-[10px]">✓ auth</span>
              )}
              {info?.auth_ok === false && (
                <span className="text-yellow-400 text-[10px]">! auth</span>
              )}
            </div>
            {!info?.installed && (
              <button
                onClick={() => install(tool)}
                disabled={installing === tool}
                className="px-2 py-0.5 text-[10px] rounded bg-accent/20 text-accent
                           hover:bg-accent/30 disabled:opacity-50"
              >
                {installing === tool ? '...' : 'Install'}
              </button>
            )}
          </div>
        );
      })}

      {Object.keys(tools).length === 0 && !probing && (
        <p className="text-xs text-text-secondary italic">
          Click "Scan" to detect installed tools.
        </p>
      )}
    </div>
  );
}
