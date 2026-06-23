/**
 * ErrorBoundary — catches render-time crashes and shows the error instead of a
 * blank (black) screen.
 *
 * This exists because the window is created hidden and revealed only after the
 * dark UI paints (see main.tsx). Without a boundary, any render crash leaves
 * just the dark native background — indistinguishable from a hang. Here we make
 * the failure visible and copyable so it can actually be diagnosed.
 */
import React from 'react';

interface State {
  error: Error | null;
  info: string | null;
}

export class ErrorBoundary extends React.Component<{ children: React.ReactNode }, State> {
  state: State = { error: null, info: null };

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    this.setState({ info: info.componentStack ?? null });
    // Also log to console so it shows up in the WebView devtools.
    console.error('[ErrorBoundary] render crash:', error, info);
  }

  render() {
    const { error, info } = this.state;
    if (!error) return this.props.children;

    return (
      <div
        style={{
          position: 'fixed',
          inset: 0,
          background: '#0d1117',
          color: '#e6edf3',
          padding: 24,
          overflow: 'auto',
          fontFamily: "'Cascadia Code', 'JetBrains Mono', monospace",
          fontSize: 13,
          lineHeight: 1.5,
        }}
      >
        <div style={{ color: '#ff7b72', fontWeight: 600, marginBottom: 12 }}>
          应用启动时崩溃 / App crashed on startup
        </div>
        <div style={{ color: '#ffa198', whiteSpace: 'pre-wrap', marginBottom: 16 }}>
          {error.name}: {error.message}
        </div>
        {error.stack && (
          <pre style={{ color: '#8b949e', whiteSpace: 'pre-wrap', margin: 0, marginBottom: 16 }}>
            {error.stack}
          </pre>
        )}
        {info && (
          <pre style={{ color: '#6e7681', whiteSpace: 'pre-wrap', margin: 0 }}>
            {info}
          </pre>
        )}
        <button
          onClick={() => location.reload()}
          style={{
            marginTop: 16,
            padding: '6px 14px',
            background: '#238636',
            color: '#fff',
            border: 'none',
            borderRadius: 6,
            cursor: 'pointer',
          }}
        >
          重新加载 / Reload
        </button>
      </div>
    );
  }
}
