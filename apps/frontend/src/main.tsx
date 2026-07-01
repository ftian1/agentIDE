import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './App';
import './styles/index.css';
import './lib/monacoSetup';
import { ErrorBoundary } from './components/ErrorBoundary';
import { initApprovalListeners } from './stores/approvalStore';
import { initAgentListeners } from './stores/agentStore';
import { initAgentLogListeners } from './stores/agentLogStore';
import { initPerfListeners } from './stores/perfStore';
import { initHttpTrafficListeners } from './stores/httpTrafficStore';
import { initFileTreeListeners } from './stores/fileTreeCacheStore';
import { initHttpEventBridge } from './lib/httpEventBridge';

// Startup: a small native splash window (Rust-side, centered, frameless)
// is shown first.  The main IDE window starts hidden.  Once React renders,
// we call `frontend_ready` to close the splash and reveal the main window.
// A 10 s Rust-side fallback prevents permanent-hidden-window deadlock if
// JS crashes — same safety as the old ErrorBoundary approach.

// Fade out the inline HTML splash overlay (safety net inside the main
// window, in case the native splash window failed to create).
const splash = document.getElementById('splash');
if (splash) {
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      splash.style.opacity = '0';
      splash.addEventListener('transitionend', () => splash.remove(), { once: true });
    });
  });
}

// Notify Rust that React is ready — closes the native splash window
// and shows the main IDE window.
import('@tauri-apps/api/core').then(({ invoke }) => {
  invoke('frontend_ready').catch(() => {});
});

// Wire backend → frontend event relays once, before first render. Each is
// isolated: a failure in one listener must not abort module load (which would
// blank the whole app).
function safeInit(name: string, fn: () => void) {
  try {
    fn();
  } catch (e) {
    console.error(`[init] ${name} failed:`, e);
  }
}
safeInit('approval', initApprovalListeners);
safeInit('agent', initAgentListeners);
safeInit('agentLog', initAgentLogListeners);
safeInit('perf', initPerfListeners);
safeInit('httpTraffic', initHttpTrafficListeners);
safeInit('httpBridge', initHttpEventBridge);
safeInit('fileTreeCache', initFileTreeListeners);

// Load persisted config from SQLite (async, non-blocking).
import('./stores/layoutStore').then(({ useLayoutStore }) => { useLayoutStore.getState()._init(); });
import('./stores/agentEngineStore').then(({ useAgentEngineStore }) => { useAgentEngineStore.getState()._init(); });

ReactDOM.createRoot(document.getElementById('root')!).render(
  React.createElement(
    React.StrictMode,
    null,
    React.createElement(ErrorBoundary, null, React.createElement(App, null)),
  ),
);
