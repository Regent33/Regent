// Parse `git diff` output into per-file line lists the panel can render with
// the same red/teal treatment the chat's tool-call diffs use.
//
// A real parser rather than a regex over the whole blob: the file list, the
// per-file counts, and the +/- classification all come from the same pass, and
// git's own headers (diff --git, index, ---/+++) must not leak into the
// rendered body.

export type DiffLineKind = 'context' | 'added' | 'removed' | 'hunk';

export interface DiffLine {
  readonly kind: DiffLineKind;
  readonly text: string;
}

export interface DiffFile {
  readonly path: string;
  readonly lines: readonly DiffLine[];
  readonly adds: number;
  readonly dels: number;
}

/** `diff --git a/x b/y` → `y` (the post-change path; a rename shows where the
 * file ended up, which is what the user clicks to open). */
function pathFromHeader(line: string): string | undefined {
  const match = /^diff --git a\/(.+?) b\/(.+)$/.exec(line);
  return match?.[2] ?? match?.[1];
}

export function parseUnifiedDiff(diff: string): DiffFile[] {
  const files: DiffFile[] = [];
  let path: string | undefined;
  let lines: DiffLine[] = [];
  let adds = 0;
  let dels = 0;

  const flush = () => {
    if (path === undefined) return;
    files.push({ path, lines, adds, dels });
    lines = [];
    adds = 0;
    dels = 0;
  };

  for (const raw of diff.split('\n')) {
    if (raw.startsWith('diff --git ')) {
      flush();
      path = pathFromHeader(raw);
      continue;
    }
    if (path === undefined) continue;
    // Metadata git emits between the header and the hunks.
    if (
      raw.startsWith('index ') ||
      raw.startsWith('--- ') ||
      raw.startsWith('+++ ') ||
      raw.startsWith('new file mode') ||
      raw.startsWith('deleted file mode') ||
      raw.startsWith('similarity index') ||
      raw.startsWith('rename from') ||
      raw.startsWith('rename to') ||
      raw.startsWith('old mode') ||
      raw.startsWith('new mode')
    ) {
      continue;
    }
    if (raw.startsWith('@@')) {
      lines.push({ kind: 'hunk', text: raw });
      continue;
    }
    if (raw.startsWith('+')) {
      adds += 1;
      lines.push({ kind: 'added', text: raw.slice(1) });
      continue;
    }
    if (raw.startsWith('-')) {
      dels += 1;
      lines.push({ kind: 'removed', text: raw.slice(1) });
      continue;
    }
    // "\ No newline at end of file" is git commentary, not content.
    if (raw.startsWith('\\')) continue;
    if (raw === '') continue;
    lines.push({ kind: 'context', text: raw.startsWith(' ') ? raw.slice(1) : raw });
  }
  flush();
  return files;
}
