/**
 * TerminalInstance — a single xterm.js terminal attached to a remote session.
 *
 * Creates an xterm Terminal, subscribes to streaming data from the
 * Tauri backend, and sends user keystrokes back via the TerminalApi.
 */
import { useEffect, useRef, useCallback } from 'react';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebLinksAddon } from '@xterm/addon-web-links';
import '@xterm/xterm/css/xterm.css';

import type { TerminalApi } from '../../api/terminalApi';

interface Props {
  sessionId: string;
  api: TerminalApi;
  /** Called when the terminal is ready with initial dimensions. */
  onReady?: (cols: number, rows: number) => void;
}

export function TerminalInstance({ sessionId, api, onReady }: Props) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);

  // Initialize xterm on mount
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const term = new Terminal({
      cursorBlink: true,
      cursorStyle: 'bar',
      fontSize: 14,
      fontFamily: "'Cascadia Code', 'JetBrains Mono', 'Fira Code', 'Consolas', monospace",
      theme: {
        background: '#0d1117',
        foreground: '#e6edf3',
        cursor: '#58a6ff',
        selectionBackground: '#264f78',
        black: '#484f58',
        red: '#ff7b72',
        green: '#3fb950',
        yellow: '#d29922',
        blue: '#58a6ff',
        magenta: '#bc8cff',
        cyan: '#39c5cf',
        white: '#b1bac4',
        brightBlack: '#6e7681',
        brightRed: '#ffa198',
        brightGreen: '#56d364',
        brightYellow: '#e3b341',
        brightBlue: '#79c0ff',
        brightMagenta: '#d2a8ff',
        brightCyan: '#56d4dd',
        brightWhite: '#f0f6fc',
      },
      allowProposedApi: true,
      allowTransparency: false,
      scrollback: 10000,
    });

    const fitAddon = new FitAddon();
    const webLinksAddon = new WebLinksAddon();

    term.loadAddon(fitAddon);
    term.loadAddon(webLinksAddon);

    term.open(container);
    fitAddon.fit();

    terminalRef.current = term;
    fitAddonRef.current = fitAddon;

    // Report initial dimensions
    onReady?.(term.cols, term.rows);

    // Subscribe to terminal data for this session
    const unlisten = api.onData((sid, data, _seq) => {
      if (sid === sessionId) {
        term.write(data);
      }
    });

    // Forward user keystrokes to the backend
    const keyDispose = term.onData((input) => {
      api.write(sessionId, input);
    });

    // Forward resize events
    const resizeDispose = term.onResize(({ cols, rows }) => {
      api.resize(sessionId, cols, rows);
    });

    return () => {
      unlisten();
      keyDispose.dispose();
      resizeDispose.dispose();
      term.dispose();
    };
  }, [sessionId]); // eslint-disable-line react-hooks/exhaustive-deps

  // Refit on container resize
  const handleResize = useCallback(() => {
    fitAddonRef.current?.fit();
  }, []);

  useEffect(() => {
    const observer = new ResizeObserver(() => handleResize());
    const container = containerRef.current;
    if (container) observer.observe(container);
    return () => observer.disconnect();
  }, [handleResize]);

  return (
    <div
      ref={containerRef}
      className="h-full w-full"
      style={{ overflow: 'hidden' }}
    />
  );
}
