import { useState } from 'react';
import { useConnectionStore } from '../../stores/connectionStore';

interface Props {
  onClose: () => void;
}

/** Modal dialog for creating a new SSH connection. */
export function ConnectionDialog({ onClose }: Props) {
  const [host, setHost] = useState('');
  const [port, setPort] = useState(22);
  const [user, setUser] = useState('');
  const [authMethod, setAuthMethod] = useState<'key' | 'password' | 'agent'>('key');
  const [password, setPassword] = useState('');
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const connect = useConnectionStore((s) => s.connect);

  const handleConnect = async () => {
    if (!host || !user) return;
    setConnecting(true);
    setError(null);
    try {
      await connect({
        host,
        port,
        user,
        authMethod,
        password: authMethod === 'password' ? password : undefined,
      });
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setConnecting(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
      <div className="bg-bg-secondary border border-border rounded-lg w-[480px] shadow-2xl">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-border">
          <h2 className="text-sm font-semibold">New SSH Connection</h2>
          <button onClick={onClose} className="text-text-secondary hover:text-text-primary">
            ✕
          </button>
        </div>

        {/* Body */}
        <div className="p-4 space-y-3">
          <div className="grid grid-cols-3 gap-3">
            <div className="col-span-2">
              <label className="text-xs text-text-secondary block mb-1">Host</label>
              <input
                type="text"
                value={host}
                onChange={(e) => setHost(e.target.value)}
                placeholder="e.g. 192.168.1.100"
                className="w-full bg-bg-tertiary text-text-primary text-sm px-2 py-1.5 rounded border border-border
                           focus:outline-none focus:border-accent placeholder:text-text-secondary"
              />
            </div>
            <div>
              <label className="text-xs text-text-secondary block mb-1">Port</label>
              <input
                type="number"
                value={port}
                onChange={(e) => setPort(Number(e.target.value))}
                className="w-full bg-bg-tertiary text-text-primary text-sm px-2 py-1.5 rounded border border-border
                           focus:outline-none focus:border-accent"
              />
            </div>
          </div>

          <div>
            <label className="text-xs text-text-secondary block mb-1">Username</label>
            <input
              type="text"
              value={user}
              onChange={(e) => setUser(e.target.value)}
              placeholder="e.g. root"
              className="w-full bg-bg-tertiary text-text-primary text-sm px-2 py-1.5 rounded border border-border
                         focus:outline-none focus:border-accent placeholder:text-text-secondary"
            />
          </div>

          <div>
            <label className="text-xs text-text-secondary block mb-1">Auth Method</label>
            <div className="flex gap-2">
              {(['key', 'password', 'agent'] as const).map((m) => (
                <button
                  key={m}
                  onClick={() => setAuthMethod(m)}
                  className={`px-3 py-1 text-xs rounded border transition-colors ${
                    authMethod === m
                      ? 'border-accent bg-accent/20 text-text-primary'
                      : 'border-border bg-bg-tertiary text-text-secondary hover:text-text-primary'
                  }`}
                >
                  {m === 'key' ? 'SSH Key' : m === 'password' ? 'Password' : 'Agent'}
                </button>
              ))}
            </div>
          </div>

          {authMethod === 'password' && (
            <div>
              <label className="text-xs text-text-secondary block mb-1">Password</label>
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                className="w-full bg-bg-tertiary text-text-primary text-sm px-2 py-1.5 rounded border border-border
                           focus:outline-none focus:border-accent"
              />
            </div>
          )}

          {error && (
            <div className="p-2 rounded bg-red-900/30 border border-red-700 text-red-300 text-xs">
              {error}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex justify-end gap-2 px-4 py-3 border-t border-border">
          <button
            onClick={onClose}
            className="px-3 py-1.5 text-xs rounded bg-bg-tertiary hover:bg-border text-text-secondary"
          >
            Cancel
          </button>
          <button
            onClick={handleConnect}
            disabled={connecting || !host || !user}
            className="px-4 py-1.5 text-xs rounded bg-accent text-white
                       hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {connecting ? 'Connecting...' : 'Connect'}
          </button>
        </div>
      </div>
    </div>
  );
}
