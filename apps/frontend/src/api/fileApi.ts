/**
 * File API — Tauri command wrappers for remote file read/write + git.
 */
import { invoke } from '@tauri-apps/api/core';

export interface GitBranchInfo {
  isRepo: boolean;
  current: string;
  branches: string[];
}

export interface FileApi {
  readFile: (connectionId: string, path: string) => Promise<string>;
  writeFile: (connectionId: string, path: string, content: string) => Promise<void>;
  gitBranches: (connectionId: string, path: string) => Promise<GitBranchInfo>;
  gitCheckout: (connectionId: string, path: string, branch: string) => Promise<void>;
}

export function createFileApi(): FileApi {
  return {
    readFile: (connectionId, path) =>
      invoke<string>('read_file', { connectionId, path }),
    writeFile: (connectionId, path, content) =>
      invoke<void>('write_file', { connectionId, path, content }),
    gitBranches: (connectionId, path) =>
      invoke<GitBranchInfo>('git_branches', { connectionId, path }),
    gitCheckout: (connectionId, path, branch) =>
      invoke<void>('git_checkout', { connectionId, path, branch }),
  };
}
