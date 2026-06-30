/** API wrappers for the graceful-restart Tauri commands. */

import { invoke } from '@tauri-apps/api/core';

/** Save state and exit the process — loader.exe will re-launch us. */
export async function prepareRestart(): Promise<void> {
  return invoke('prepare_restart');
}

/** Check whether a restart flag exists from a previous graceful restart. */
export async function checkRestartFlag(): Promise<boolean> {
  return invoke('check_restart_flag');
}

/**
 * Apply a downloaded self-update (.loader.exe.new) and restart.
 * Replaces the running exe with the new one and relaunches.
 */
export async function applyUpdateAndRestart(): Promise<void> {
  return invoke('apply_update_and_restart');
}
