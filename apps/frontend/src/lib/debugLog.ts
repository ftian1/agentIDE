/**
 * Shared debug logger — writes to console.log AND the AgentStdout panel.
 *
 * All critical-path logs (session spawn, terminal I/O, view switches) should
 * use this instead of raw console.log so they're visible in the bottom panel
 * even on Windows where the browser console isn't open.
 */
import { useAgentLogStore } from '../stores/agentLogStore';

export function log(channel: 'session' | 'agent' | 'system', msg: string) {
  const prefix = channel === 'agent' ? '[agent]' : channel === 'session' ? '[session]' : '[system]';
  const full = `${prefix} ${msg}`;
  console.log(full);
  try {
    useAgentLogStore.getState()._push(channel === 'system' ? 'agent' : channel, full);
  } catch {
    // Store not initialized yet — ignore
  }
}

export function logSpawn(sessionId: string, connId: string, tool: string, args: string[], env?: Record<string, string>) {
  log('system', `▶ SPAWN session=${sessionId.slice(0,8)} conn=${connId.slice(0,8)} tool=${tool}`);
  log('system', `  args: ${args.join(' ')}`);
  if (env) {
    for (const [k, v] of Object.entries(env)) {
      if (k.startsWith('ANTHROPIC_') || k.startsWith('OPENAI_') || k === 'TERM'
          || k.startsWith('__tap') || k.startsWith('__gateway') || k.startsWith('__providers')) {
        const display = k.startsWith('__providers') ? `${v.slice(0, 80)}…` : v;
        log('system', `  env: ${k}=${display}`);
      }
    }
  }
}

export function logWrite(_sessionId: string, _data: string) {
  // Terminal I/O is too noisy for the default log level.
  // Enable by uncommenting if debugging terminal issues:
  // const preview = _data.length <= 60 ? JSON.stringify(_data) : JSON.stringify(_data.slice(0, 40)) + '…';
  // log('system', `⌨ WRITE session=${_sessionId.slice(0,8)} len=${_data.length} data=${preview}`);
}

export function logTerminalData(_sessionId: string, _len: number, _preview: string) {
  // Terminal output is too noisy for the default log level.
  // Enable by uncommenting if debugging terminal issues:
  // log('system', `◀ DATA session=${_sessionId.slice(0,8)} len=${_len} preview=${_preview.slice(0, 60)}`);
}

export function logViewSwitch(view: string) {
  log('system', `↻ VIEW: ${view}`);
}
