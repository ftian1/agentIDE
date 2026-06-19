import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './App';
import './styles/index.css';

/** Simple error boundary — shows React rendering errors visibly instead of a black screen. */
class ErrorBoundary extends React.Component<
  { children: React.ReactNode },
  { error: Error | null }
> {
  constructor(props: { children: React.ReactNode }) {
    super(props);
    this.state = { error: null };
  }

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    console.error('[App] React render error:', error);
    console.error('[App] Component stack:', info.componentStack);
    // Also write to a visible DOM element for Tauri WebView debugging
    const root = document.getElementById('root');
    if (root) {
      root.innerHTML = `
        <div style="padding:20px;color:#ff7b72;background:#0d1117;font-family:monospace;font-size:13px;">
          <h2 style="color:#ff7b72;">App Crashed</h2>
          <pre style="white-space:pre-wrap;word-break:break-all;">${error.message}
${error.stack}</pre>
          <p style="color:#8b949e;margin-top:12px;">Check the browser console (F12) for details.</p>
        </div>`;
    }
  }

  render() {
    if (this.state.error) {
      return null; // error UI already written in componentDidCatch
    }
    return this.props.children;
  }
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);
