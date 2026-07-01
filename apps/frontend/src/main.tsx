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

// ── Startup profiling ──────────────────────────────────────────────
// Each milestone is queued during sync module eval, then flushed
// asynchronously once the Tauri IPC bridge is guaranteed ready.
// This avoids the silent-failure case where invoke() is called before
// window.__TAURI_INTERNALS__ is injected.
const t0 = performance.now();
const marks: Record<string, number> = {};
const pending: Array<{ name: string; ms: number }> = [];
let invokeReady: ((cmd: string, args?: Record<string, unknown>) => Promise<unknown>) | null = null;

function mark(label: string) {
  const ms = +(performance.now() - t0).toFixed(1);
  marks[label] = ms;
  pending.push({ name: label, ms });
}

// Deferred IPC setup: dynamic import ensures the module resolves after
// Tauri's initialization script has injected __TAURI_INTERNALS__.
const ipcReady = import('@tauri-apps/api/core').then(({ invoke }) => {
  invokeReady = invoke;
  // Flush queued milestones
  for (const m of pending) {
    invoke('frontend_milestone', { name: m.name, ms: m.ms }).catch(() => {});
  }
  pending.length = 0;
}).catch((e: unknown) => {
  console.error('[startup] failed to import @tauri-apps/api/core:', e);
});

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

mark('imports');

// Wire backend → frontend event relays once, before first render. Each is
// isolated: a failure in one listener must not abort module load (which would
// blank the whole app).
function safeInit(name: string, fn: () => void) {
  const s = performance.now();
  try {
    fn();
  } catch (e) {
    console.error(`[init] ${name} failed:`, e);
  }
  mark('init:' + name);
}
safeInit('approval', initApprovalListeners);
safeInit('agent', initAgentListeners);
safeInit('agentLog', initAgentLogListeners);
safeInit('perf', initPerfListeners);
safeInit('httpTraffic', initHttpTrafficListeners);
safeInit('httpBridge', initHttpEventBridge);
safeInit('fileTreeCache', initFileTreeListeners);

mark('listeners');

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

mark('first-render');

// Notify Rust that React is ready — closes the native splash window
// and shows the main IDE window.  Send timing breakdown for the log.
mark('frontend-ready');

// Wait for IPC to be ready, flush any remaining milestones, then notify Rust.
ipcReady.then(() => {
  // Flush any milestones queued after the initial batch
  if (invokeReady) {
    for (const m of pending) {
      invokeReady('frontend_milestone', { name: m.name, ms: m.ms }).catch(() => {});
    }
    pending.length = 0;
    invokeReady('frontend_ready', { timings: marks }).catch((e: unknown) => {
      console.error('[startup] frontend_ready failed:', e);
    });
  }
});
