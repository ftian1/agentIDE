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
  /**
   * Whether this terminal is currently visible. When it lives in a hidden tab,
   * xterm can't measure its container (size 0) so fit() computes wrong columns
   * and the agent CLI TUI wraps/garbles. Flipping this to true triggers a
   * refit + PTY resize once the container has real dimensions.
   */
  active?: boolean;
}

export function TerminalInstance({ sessionId, api, onReady, active = true }: Props) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);

  // Initialize xterm on mount
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const fitAddon = new FitAddon();

    const term = new Terminal({
      cursorBlink: true,
      cursorStyle: 'bar',
      fontSize: 14,
      // Lead with Consolas: it ships on every Windows install. The previous
      // first choices (Cascadia Code / JetBrains Mono / Fira Code) are usually
      // NOT installed, so xterm measured glyph widths against a font that
      // wasn't there while actually rendering with the monospace fallback —
      // the measure/render mismatch is what made the letter spacing look wrong.
      fontFamily: "Consolas, 'Cascadia Mono', 'Courier New', monospace",
      letterSpacing: 0,
      lineHeight: 1.0,
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

    const webLinksAddon = new WebLinksAddon();
    term.loadAddon(fitAddon);
    term.loadAddon(webLinksAddon);

    term.open(container);

    // Fit AFTER the custom font is actually loaded. If we measure character
    // cell width before the font is ready, xterm sizes cells against the
    // fallback font; once the real font swaps in, glyph widths no longer match
    // the cells → garbled letter spacing (the "first launch looks wrong, fine
    // after reconnect" symptom — by reconnect the font is cached). clearing the
    // texture atlas forces xterm to remeasure with the now-loaded font.
    const fitWhenFontReady = () => {
      const doFit = () => {
        try {
          term.clearTextureAtlas?.();
          fitAddon.fit();
        } catch {}
        onReady?.(term.cols, term.rows);
      };
      const fonts = (document as Document & { fonts?: FontFaceSet }).fonts;
      if (fonts?.ready) {
        fonts.ready.then(() => requestAnimationFrame(doFit)).catch(() => requestAnimationFrame(doFit));
      } else {
        requestAnimationFrame(doFit);
      }
    };
    fitWhenFontReady();

    terminalRef.current = term;
    fitAddonRef.current = fitAddon;

    // Subscribe to terminal data for this session
    const unlisten = api.onData((sid, data, _seq) => {
      if (sid === sessionId) {
        term.write(data);
      }
    });

    // Forward user keystrokes to the backend
    const keyDispose = term.onData((input) => {
      api.write(sessionId, input).catch((e) => {
        console.error(`[Terminal] write_input failed for ${sessionId}:`, e);
      });
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

  // Refit on container resize — use rAF to let layout settle
  const handleResize = useCallback(() => {
    requestAnimationFrame(() => {
      try { fitAddonRef.current?.fit(); } catch {}
    });
  }, []);

  useEffect(() => {
    const observer = new ResizeObserver(() => handleResize());
    const container = containerRef.current;
    if (container) observer.observe(container);
    return () => observer.disconnect();
  }, [handleResize]);

  // When the terminal becomes visible (e.g. switching to the raw tab), its
  // container finally has real dimensions — refit and push the new size to the
  // PTY so the agent CLI re-renders at the correct column count.
  // Also focus the terminal so keyboard input works immediately.
  useEffect(() => {
    if (!active) return;
    const id = requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        try {
          const term = terminalRef.current;
          if (term) {
            const ff = term.options.fontFamily;
            term.options.fontFamily = `${ff} `;
            term.options.fontFamily = ff;
            term.clearTextureAtlas?.();
            term.focus();
          }
          fitAddonRef.current?.fit();
          if (term) api.resize(sessionId, term.cols, term.rows);
        } catch {}
      }),
    );
    return () => cancelAnimationFrame(id);
  }, [active, api, sessionId]);

  return (
    <div
      ref={containerRef}
      className="h-full w-full"
    />
  );
}
