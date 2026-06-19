import { useState } from 'react';

interface Props {
  onClose: () => void;
}

/** Modal dialog for creating a new SSH connection. */
export function ConnectionDialog({ onClose }: Props) {
  const [host, setHost] = useState('');
  const [port, setPort] = useState(22);
  const [user, setUser] = useState('');
  const [authMethod, setAuthMethod] = useState('key');

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
      <div className="bg-bg-secondary border border-border rounded-lg w-[480px] shadow-2xl">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-border">
          <h2 className="text-sm font-semibold">New Connection</h2>
          <button onClick={onClose} className="text-text-secondary hover:text-text-primary">
            ✕
          </button>
        </div>

        {/* Body (placeholder) */}
        <div className="p-4 space-y-3">
          <p className="text-text-secondary text-xs">
            SSH connection dialog — Phase 6 will add full form with host, port,
            user, auth method, SSH config loader.
          </p>
        </div>

        {/* Footer */}
        <div className="flex justify-end gap-2 px-4 py-3 border-t border-border">
          <button
            onClick={onClose}
            className="px-3 py-1.5 text-xs rounded bg-bg-tertiary hover:bg-border text-text-secondary"
          >
            Cancel
          </button>
          <button className="px-3 py-1.5 text-xs rounded bg-accent text-white">
            Connect
          </button>
        </div>
      </div>
    </div>
  );
}
