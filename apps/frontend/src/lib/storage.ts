/**
 * SQLite-backed persistence utility.
 *
 * Replaces localStorage with the Tauri SQLite settings KV store so that
 * configuration survives origin changes (e.g. dev-server mode).
 * Falls back to localStorage when Tauri is unavailable (browser dev).
 */
import { invoke } from '@tauri-apps/api/core';

/**
 * Load persisted JSON from SQLite.
 * @returns The parsed value, or `fallback` if nothing saved / Tauri unavailable.
 */
export async function loadPersisted<T>(key: string, fallback: T): Promise<T> {
  try {
    const raw: string | null = await invoke('load_app_config', { key });
    if (raw) return JSON.parse(raw) as T;
  } catch {
    // Tauri unavailable or key not found — try localStorage fallback
    try {
      const local = localStorage.getItem(key);
      if (local) return JSON.parse(local) as T;
    } catch { /* ignore */ }
  }
  return fallback;
}

/**
 * Save a JSON-serialisable value to SQLite (and localStorage for
 * dev-server offline fallback).
 */
export async function savePersisted(key: string, value: unknown): Promise<void> {
  const json = JSON.stringify(value);
  // Always save to localStorage so values survive Tauri restart races.
  try { localStorage.setItem(key, json); } catch { /* ignore */ }
  // Primary: save to SQLite.
  try { await invoke('save_app_config', { key, value: json }); } catch { /* ignore */ }
}
