/**
 * UpdateBanner — shows a non-intrusive notification when the background
 * OTA updater has downloaded new components and a restart would apply them.
 *
 * Listens for the Tauri `update:available` event.  On click, calls
 * `prepare_restart` to save state and exit gracefully.
 */

import { useEffect, useState } from 'react';
import { RefreshCw, X, Download, CheckCircle } from 'lucide-react';
import { prepareRestart } from '../../api/restartApi';

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

  const handleRestart = async () => {
    setRestarting(true);
    try {
      await prepareRestart();
    } catch (e) {
      console.error('[UpdateBanner] prepareRestart failed:', e);
      setRestarting(false);
    }
    // prepareRestart calls process::exit — we won't reach here on success.
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

  // Show "update ready" banner.
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
