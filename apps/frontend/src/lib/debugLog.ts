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
          || k.startsWith('__tap') || k.startsWith('__gateway')) {
        log('system', `  env: ${k}=${v}`);
      }
    }
  }
}

export function logWrite(sessionId: string, data: string) {
  const preview = data.length <= 60 ? JSON.stringify(data) : JSON.stringify(data.slice(0, 40)) + '…';
  log('system', `⌨ WRITE session=${sessionId.slice(0,8)} len=${data.length} data=${preview}`);
}

export function logTerminalData(sessionId: string, len: number, preview: string) {
  log('system', `◀ DATA session=${sessionId.slice(0,8)} len=${len} preview=${preview.slice(0, 60)}`);
}

export function logViewSwitch(view: string) {
  log('system', `↻ VIEW: ${view}`);
}
