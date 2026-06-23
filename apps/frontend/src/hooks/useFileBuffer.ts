/**
 * useFileBuffer — headless view-model for an open file buffer.
 */
import { useEffect } from 'react';
import { useFileBufferStore, bufferKey } from '../stores/fileBufferStore';
import type { FileBuffer } from '../stores/fileBufferStore';

export interface FileBufferView {
  buffer: FileBuffer | null;
  dirty: boolean;
  edit: (draft: string) => void;
  save: () => void;
}

export function useFileBuffer(
  connectionId: string | null,
  path: string | null,
): FileBufferView {
  const key = connectionId && path ? bufferKey(connectionId, path) : null;
  const buffer = useFileBufferStore((s) => (key ? s.buffers[key] ?? null : null));
  const open = useFileBufferStore((s) => s.open);
  const editFn = useFileBufferStore((s) => s.edit);
  const saveFn = useFileBufferStore((s) => s.save);

  useEffect(() => {
    if (connectionId && path) open(connectionId, path);
  }, [connectionId, path, open]);

  return {
    buffer,
    dirty: buffer ? buffer.draft !== buffer.original : false,
    edit: (draft: string) => key && editFn(key, draft),
    save: () => key && saveFn(key),
  };
}
