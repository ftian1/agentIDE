/**
 * File API — Tauri command wrappers for remote file read/write.
 */
import { invoke } from '@tauri-apps/api/core';

export interface FileApi {
  readFile: (connectionId: string, path: string) => Promise<string>;
  writeFile: (connectionId: string, path: string, content: string) => Promise<void>;
}

export function createFileApi(): FileApi {
  return {
    readFile: (connectionId, path) =>
      invoke<string>('read_file', { connectionId, path }),
    writeFile: (connectionId, path, content) =>
      invoke<void>('write_file', { connectionId, path, content }),
  };
}
