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

// The window is created visible with a dark native backgroundColor (#0d1117),
// which alone prevents the white WebView2 cold-start flash. We deliberately do
// NOT hide the window and reveal it from JS: that coupled the window's
// visibility to JS succeeding, so any startup crash turned into a permanent
// black screen. Now a crash shows the ErrorBoundary instead.

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

ReactDOM.createRoot(document.getElementById('root')!).render(
  React.createElement(
    React.StrictMode,
    null,
    React.createElement(ErrorBoundary, null, React.createElement(App, null)),
  ),
);
