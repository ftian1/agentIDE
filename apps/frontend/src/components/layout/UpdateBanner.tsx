/**
 * UpdateBanner / UpgradeDialog — shows a notification when the background
 * OTA updater has downloaded new components.
 *
 * - loader.exe self-update → modal dialog (Upgrade / Cancel)
 * - Other file updates → non-intrusive banner with Restart button
 *
 * Listens for the Tauri `update:available` event.
 */

import { useEffect, useState } from 'react';
import { RefreshCw, X, Download, CheckCircle, AlertTriangle } from 'lucide-react';
import { prepareRestart, applyUpdateAndRestart } from '../../api/restartApi';

interface UpdatePayload {
  version: string;
  updated: string[];
}

interface ProgressPayload {
  file: string;
  downloaded: number;
  total: number;
}

export function UpdateBanner() {
  const [available, setAvailable] = useState<UpdatePayload | null>(null);
  const [restarting, setRestarting] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  const [downloading, setDownloading] = useState<ProgressPayload | null>(null);

  useEffect(() => {
    import('@tauri-apps/api/event').then(({ listen }) => {
      // Update available — all downloads complete.
      listen<UpdatePayload>('update:available', (event) => {
        console.log('[UpdateBanner] update available:', event.payload);
        setAvailable(event.payload);
        setDismissed(false);
      });

      // Download progress for individual files.
      listen<ProgressPayload>('update:progress', (event) => {
        console.log('[UpdateBanner] progress:', event.payload);
        setDownloading(event.payload);
      });
    });
  }, []);

  const isSelfUpdate = available?.updated.includes('loader.exe') ?? false;

  const handleRestart = () => {
    setRestarting(true);
    if (isSelfUpdate) {
      applyUpdateAndRestart().catch(() => {});
    } else {
      prepareRestart().catch(() => {});
    }
  };

  const handleDismiss = () => {
    setDismissed(true);
  };

  // Show download progress banner.
  if (downloading && !available) {
    return (
      <div className="flex items-center gap-2 px-3 py-1.5 bg-blue-900/30 border-b border-blue-800/50 text-xs text-blue-200">
        <Download size={14} className="animate-pulse" />
        <span>Downloading {downloading.file}…</span>
        <span className="text-blue-300/70">
          {downloading.downloaded > 0
            ? `${(downloading.downloaded / 1024).toFixed(0)} KB`
            : ''}
          {downloading.total > 0
            ? ` / ${(downloading.total / 1024).toFixed(0)} KB`
            : ''}
        </span>
      </div>
    );
  }

  // Self-update (loader.exe) → modal dialog.
  if (available && !dismissed && isSelfUpdate) {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
        <div className="bg-bg-secondary border border-border rounded-lg shadow-xl w-96 p-6">
          <div className="flex items-start gap-3 mb-4">
            <AlertTriangle size={24} className="text-amber-400 flex-shrink-0 mt-0.5" />
            <div>
              <h3 className="text-sm font-semibold text-text-primary">
                IDE Update Available
              </h3>
              <p className="text-xs text-text-secondary mt-1">
                A new version of Remote AI IDE has been downloaded and is ready
                to install. The IDE will restart to apply the update.
              </p>
              {available.updated.length > 1 && (
                <p className="text-xs text-text-secondary mt-1">
                  Also updated: {available.updated.filter(f => f !== 'loader.exe').join(', ')}
                </p>
              )}
            </div>
          </div>

          <div className="flex justify-end gap-2">
            <button
              onClick={handleDismiss}
              disabled={restarting}
              className="px-4 py-1.5 text-xs rounded text-text-secondary hover:text-text-primary hover:bg-bg-tertiary transition-colors disabled:opacity-50"
            >
              Cancel
            </button>
            <button
              onClick={handleRestart}
              disabled={restarting}
              className="flex items-center gap-1.5 px-4 py-1.5 text-xs rounded bg-accent text-white hover:bg-blue-500 disabled:opacity-50 transition-colors"
            >
              <RefreshCw size={12} className={restarting ? 'animate-spin' : ''} />
              {restarting ? 'Upgrading…' : 'Upgrade & Restart'}
            </button>
          </div>
        </div>
      </div>
    );
  }

  // Regular update (frontend / agent / etc.) → inline banner.
  if (available && !dismissed) {
    return (
      <div className="flex items-center gap-2 px-3 py-1.5 bg-green-900/30 border-b border-green-800/50 text-xs">
        <CheckCircle size={14} className="text-green-400" />
        <span className="text-green-200">
          Update ready —{' '}
          {available.updated.slice(0, 3).join(', ')}
          {available.updated.length > 3
            ? ` +${available.updated.length - 3} more`
            : ''}
        </span>
        <div className="flex-1" />
        <button
          onClick={handleRestart}
          disabled={restarting}
          className="flex items-center gap-1 px-2 py-0.5 rounded bg-green-700/50 text-green-100 hover:bg-green-700/70 disabled:opacity-50 transition-colors"
        >
          <RefreshCw size={12} className={restarting ? 'animate-spin' : ''} />
          {restarting ? 'Restarting…' : 'Restart'}
        </button>
        <button
          onClick={handleDismiss}
          className="p-0.5 text-green-400/70 hover:text-green-300"
        >
          <X size={12} />
        </button>
      </div>
    );
  }

  return null;
}
