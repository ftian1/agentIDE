/**
 * Line-level diff for the inline patch-preview (staged changes) view.
 *
 * Produces a unified line list (context / add / del) plus a list of hunks.
 * A hunk is a maximal run of changed lines between two unchanged anchors;
 * each hunk can be accepted or rejected independently, and
 * {@link reconstruct} rebuilds file content from the original plus a set of
 * per-hunk decisions. This is the engine behind per-hunk ✓/✗.
 *
 * Pure functions only — no React, no Monaco — so it is unit-testable.
 */

export type LineType = 'context' | 'add' | 'del';

export interface UnifiedLine {
  type: LineType;
  text: string;
  /** 1-based line number in the original file, or null for added lines. */
  oldLine: number | null;
  /** 1-based line number in the proposed file, or null for deleted lines. */
  newLine: number | null;
  /** Hunk this line belongs to (null for context lines). */
  hunkId: string | null;
}

export interface Hunk {
  id: string;
  /** 0-based index of the first original line this hunk replaces. */
  oldStart: number;
  /** Number of original lines this hunk replaces (may be 0 for pure inserts). */
  oldCount: number;
  /** Replacement lines when the hunk is accepted (may be empty for pure deletes). */
  newLines: string[];
  /** Index into the unified line list where the hunk's controls render. */
  firstLineIndex: number;
}

export interface DiffResult {
  lines: UnifiedLine[];
  hunks: Hunk[];
}

type Op = { kind: 'eq' | 'del' | 'ins'; text: string };

/** Split into lines without a trailing empty element for files ending in \n. */
function splitLines(s: string): string[] {
  if (s === '') return [];
  const lines = s.split('\n');
  if (lines.length > 0 && lines[lines.length - 1] === '') lines.pop();
  return lines;
}

/**
 * LCS-based edit script over lines. O(n*m) time/space — fine for the small
 * patches agents produce. Falls back to a single replace-all hunk past a
 * size guard so a giant file can never hang the UI.
 */
function editScript(oldLines: string[], newLines: string[]): Op[] {
  const n = oldLines.length;
  const m = newLines.length;

  if (n > 4000 || m > 4000) {
    const ops: Op[] = [];
    for (const t of oldLines) ops.push({ kind: 'del', text: t });
    for (const t of newLines) ops.push({ kind: 'ins', text: t });
    return ops;
  }

  // dp[i][j] = LCS length of oldLines[i..] and newLines[j..]
  const dp: number[][] = Array.from({ length: n + 1 }, () => new Array(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] = oldLines[i] === newLines[j]
        ? dp[i + 1][j + 1] + 1
        : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }

  const ops: Op[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (oldLines[i] === newLines[j]) {
      ops.push({ kind: 'eq', text: oldLines[i] });
      i++; j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      ops.push({ kind: 'del', text: oldLines[i] });
      i++;
    } else {
      ops.push({ kind: 'ins', text: newLines[j] });
      j++;
    }
  }
  while (i < n) { ops.push({ kind: 'del', text: oldLines[i] }); i++; }
  while (j < m) { ops.push({ kind: 'ins', text: newLines[j] }); j++; }
  return ops;
}

/**
 * Compute the unified diff + hunks between original and proposed content.
 * Hunk ids are derived from their position so they are stable across renders
 * (no randomness — important for resume/replay and React keys).
 */
export function computeDiff(oldContent: string, newContent: string): DiffResult {
  const oldLines = splitLines(oldContent);
  const newLines = splitLines(newContent);
  const ops = editScript(oldLines, newLines);

  const lines: UnifiedLine[] = [];
  const hunks: Hunk[] = [];

  let oldNo = 0; // 0-based cursor into oldLines
  let newNo = 0; // 0-based cursor into newLines
  let k = 0;

  while (k < ops.length) {
    const op = ops[k];
    if (op.kind === 'eq') {
      lines.push({
        type: 'context',
        text: op.text,
        oldLine: oldNo + 1,
        newLine: newNo + 1,
        hunkId: null,
      });
      oldNo++; newNo++; k++;
      continue;
    }

    // Start of a changed run — coalesce all adjacent del/ins into one hunk.
    const hunkId = `h${hunks.length}`;
    const firstLineIndex = lines.length;
    const oldStart = oldNo;
    const replacement: string[] = [];
    let oldCount = 0;

    while (k < ops.length && ops[k].kind !== 'eq') {
      const cur = ops[k];
      if (cur.kind === 'del') {
        lines.push({
          type: 'del',
          text: cur.text,
          oldLine: oldNo + 1,
          newLine: null,
          hunkId,
        });
        oldNo++; oldCount++;
      } else {
        lines.push({
          type: 'add',
          text: cur.text,
          oldLine: null,
          newLine: newNo + 1,
          hunkId,
        });
        newNo++;
        replacement.push(cur.text);
      }
      k++;
    }

    hunks.push({ id: hunkId, oldStart, oldCount, newLines: replacement, firstLineIndex });
  }

  return { lines, hunks };
}

/**
 * Rebuild file content from the original plus per-hunk decisions.
 *
 * A hunk that is `accepted` contributes its replacement lines; any other
 * state (`rejected` or still `pending`) keeps the original lines. This makes
 * pending changes "staged but not applied" — the on-disk file only advances
 * as hunks are accepted.
 */
export function reconstruct(
  oldContent: string,
  hunks: Hunk[],
  decisions: Record<string, 'accepted' | 'rejected' | 'pending'>,
): string {
  const oldLines = splitLines(oldContent);
  const sorted = [...hunks].sort((a, b) => a.oldStart - b.oldStart);

  const out: string[] = [];
  let cursor = 0;
  for (const h of sorted) {
    // Copy untouched original lines before this hunk.
    while (cursor < h.oldStart) out.push(oldLines[cursor++]);
    if (decisions[h.id] === 'accepted') {
      out.push(...h.newLines);
    } else {
      for (let x = 0; x < h.oldCount; x++) out.push(oldLines[h.oldStart + x]);
    }
    cursor = h.oldStart + h.oldCount;
  }
  while (cursor < oldLines.length) out.push(oldLines[cursor++]);

  // Preserve a trailing newline if the original had one.
  const trailing = oldContent.endsWith('\n');
  return out.length > 0 ? out.join('\n') + (trailing ? '\n' : '') : '';
}
