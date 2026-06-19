/**
 * Hook for managing an xterm.js terminal instance lifecycle.
 * Phase 4: creates Terminal, attaches addons, subscribes to Tauri events.
 */
import { useRef, useEffect, useCallback } from 'react';

interface UseTerminalOptions {
  containerRef: React.RefObject<HTMLDivElement | null>;
  sessionId: string;
  onData: (data: string) => void;
}

export function useTerminal(_options: UseTerminalOptions) {
  // Phase 4: xterm.js integration
  const initialized = useRef(false);

  useEffect(() => {
    if (initialized.current) return;
    initialized.current = true;
    // TODO: create xterm Terminal, add fit/weblinks addons
    // TODO: subscribe to terminal:data Tauri events
    // TODO: attach onData callback for user input
  }, []);

  const write = useCallback((_data: string) => {
    // TODO: terminal.write(data)
  }, []);

  const resize = useCallback((_cols: number, _rows: number) => {
    // TODO: terminal.resize(cols, rows)
  }, []);

  return { write, resize };
}
