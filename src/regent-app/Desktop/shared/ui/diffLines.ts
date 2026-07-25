// Line-level LCS diff for the tool-call "replace" code preview — a single
// unified sequence (removed lines from `before` immediately followed by their
// replacement from `after`; unchanged lines shown once) instead of two
// separate scrollable Before/After panels. Bounded by the backend's own
// CODE_DETAIL_MAX_CHARS cap, so an O(n*m) LCS table is cheap here.

export type DiffLineKind = 'unchanged' | 'removed' | 'added';

export interface DiffLine {
  readonly kind: DiffLineKind;
  readonly text: string;
}

export function diffLines(before: string, after: string): DiffLine[] {
  const a = before.split('\n');
  const b = after.split('\n');
  const n = a.length;
  const m = b.length;

  // lcs[i][j] = length of the longest common subsequence of a[i:] and b[j:].
  const lcs: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      lcs[i][j] = a[i] === b[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
    }
  }

  const result: DiffLine[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      result.push({ kind: 'unchanged', text: a[i] });
      i += 1;
      j += 1;
    } else if (lcs[i + 1][j] >= lcs[i][j + 1]) {
      result.push({ kind: 'removed', text: a[i] });
      i += 1;
    } else {
      result.push({ kind: 'added', text: b[j] });
      j += 1;
    }
  }
  while (i < n) {
    result.push({ kind: 'removed', text: a[i] });
    i += 1;
  }
  while (j < m) {
    result.push({ kind: 'added', text: b[j] });
    j += 1;
  }
  return result;
}
