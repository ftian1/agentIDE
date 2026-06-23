/**
 * Context File Store — tracks which files are in each agent session's working
 * context, derived from the agent's tool-use stream (Read/Edit/Write events).
 *
 * Powers the "Show Active Files" dropdown. Files can be "kicked out" of the
 * displayed context; per the current design that is a UI-only removal, but the
 * action is funnelled through {@link kickFile} so it can later be upgraded to
 * actually instruct the agent without touching the UI.
 */
import { create } from 'zustand';

export type ContextFileOrigin = 'read' | 'edit' | 'write';

export interface ContextFile {
  path: string;
  origin: ContextFileOrigin;
  /** Monotonic counter of last touch, for recency ordering. */
  lastSeq: number;
}

interface ContextFileStore {
  /** session_id → (path → ContextFile). */
  bySession: Record<string, Record<string, ContextFile>>;
  /** session_id → set of paths the user kicked out (suppressed from display). */
  kicked: Record<string, Set<string>>;

  _touch: (sessionId: string, path: string, origin: ContextFileOrigin, seq: number) => void;
  kickFile: (sessionId: string, path: string) => void;
  restoreFile: (sessionId: string, path: string) => void;
  clear: (sessionId: string) => void;
}

export const useContextFileStore = create<ContextFileStore>((set) => ({
  bySession: {},
  kicked: {},

  _touch: (sessionId, path, origin, seq) =>
    set((s) => {
      const files = { ...(s.bySession[sessionId] ?? {}) };
      const prev = files[path];
      // edit/write outrank read as the recorded origin (more significant).
      const originRank = { read: 0, edit: 1, write: 1 } as const;
      const nextOrigin =
        prev && originRank[prev.origin] >= originRank[origin] ? prev.origin : origin;
      files[path] = { path, origin: nextOrigin, lastSeq: seq };
      return { bySession: { ...s.bySession, [sessionId]: files } };
    }),

  kickFile: (sessionId, path) =>
    set((s) => {
      // UI-only removal for now. Upgrade point: also send an instruction to the
      // agent here (e.g. via a tauri command) — UI contract stays the same.
      const set2 = new Set(s.kicked[sessionId] ?? []);
      set2.add(path);
      return { kicked: { ...s.kicked, [sessionId]: set2 } };
    }),

  restoreFile: (sessionId, path) =>
    set((s) => {
      const set2 = new Set(s.kicked[sessionId] ?? []);
      set2.delete(path);
      return { kicked: { ...s.kicked, [sessionId]: set2 } };
    }),

  clear: (sessionId) =>
    set((s) => {
      const bySession = { ...s.bySession };
      const kicked = { ...s.kicked };
      delete bySession[sessionId];
      delete kicked[sessionId];
      return { bySession, kicked };
    }),
}));

/** Visible (non-kicked) context files for a session, most-recent first. */
export function selectActiveFiles(
  slices: Pick<ContextFileStore, 'bySession' | 'kicked'>,
  sessionId: string | null,
): ContextFile[] {
  if (!sessionId) return [];
  const files = slices.bySession[sessionId];
  if (!files) return [];
  const kicked = slices.kicked[sessionId] ?? new Set<string>();
  return Object.values(files)
    .filter((f) => !kicked.has(f.path))
    .sort((a, b) => b.lastSeq - a.lastSeq);
}

/**
 * Parse a tool-use action label into a context-file touch.
 * Labels come from agent_parse::describe_tool_use, e.g. "Read /a/b.ts",
 * "Edit /a/b.ts", "Write /a/b.ts". Returns null for non-file tools (Bash, …).
 */
export function parseActionLabel(
  label: string | undefined,
): { origin: ContextFileOrigin; path: string } | null {
  if (!label) return null;
  const m = /^(Read|Edit|Write|read_file|write_file|edit_file)\s+(\S.*)$/.exec(label.trim());
  if (!m) return null;
  const verb = m[1].toLowerCase();
  const origin: ContextFileOrigin =
    verb.startsWith('read') ? 'read' : verb.startsWith('write') ? 'write' : 'edit';
  return { origin, path: m[2].trim() };
}
