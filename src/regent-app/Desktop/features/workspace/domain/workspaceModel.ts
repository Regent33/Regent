// Pure shapes + mappers for the coding panel. Kept free of React and of the
// RPC client so they're testable directly — this repo's hook tests only ever
// exercise pure exports (no DOM/jsdom is installed).

/** Structural, not the DOM type — same approach as devtoolsGuard's KeyChord. */
export interface SaveChord {
  readonly key: string;
  readonly ctrlKey: boolean;
  readonly metaKey: boolean;
}

/** Ctrl+S / Cmd+S. Case-folded so a capital S (shift or caps lock) still saves. */
export function isSaveShortcut(e: SaveChord): boolean {
  return (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's';
}

/** What to show after an "Open Folder" attempt: `undefined` on success, always a
 * non-empty string on failure.
 *
 * Reported 2026-07-29 — picking a folder did nothing whatsoever. The deacon the
 * app spawns is pinned by `REGENT_DEACON_PATH` to an install predating
 * `workspace.set`, so the call returned `-32601 method not found` and the
 * handler dropped it on the floor: a real error was indistinguishable from a
 * click that did nothing. The fallback string exists because a blank or missing
 * message would recreate exactly that silence. */
export function openFolderMessage(ok: boolean, message?: string): string | undefined {
  if (ok) return undefined;
  return message !== undefined && message.trim() !== ''
    ? message
    : 'Opening that folder failed.';
}

/** Which folder-picker affordance the panel header shows.
 *
 * The button used to render only while the session was still on the scratch
 * space (`isDefault`), so opening the WRONG repo was a dead end: the one control
 * that could fix it vanished the moment it was needed, and the only way out was
 * starting a new conversation and losing the thread. Reported 2026-07-30.
 *
 * A function, not an inline ternary, so the regression has a test: the point is
 * that `false` still returns a label rather than nothing. */
export function folderButtonMode(isDefault: boolean): 'open' | 'change' {
  return isDefault ? 'open' : 'change';
}

/** How long a loading indicator stays on screen at minimum, in ms.
 *
 * A local file read finishes in single-digit milliseconds, so the indicator was
 * technically correct and practically invisible — reported 2026-07-30 as "I don't
 * see the loading animation". Feedback nobody can perceive is not feedback.
 *
 * 450ms is above the ~400ms where a flash stops registering as a flash, and well
 * under the ~1s where a wait starts feeling like a stall. The cost is honest: a
 * fast open is held back by up to 450ms. That is a deliberate trade for knowing
 * the app heard the click, not an oversight.
 */
export const MIN_LOADING_MS = 450;

/** How much longer to keep an indicator up, given how long the work took.
 *
 * Pure so the arithmetic is tested rather than eyeballed — an off-by-one here
 * either reintroduces the flash or adds a delay to every single open.
 */
export function remainingHold(elapsedMs: number, minMs: number = MIN_LOADING_MS): number {
  if (!Number.isFinite(elapsedMs) || elapsedMs < 0) return minMs;
  return Math.max(0, minMs - elapsedMs);
}

export interface TreeEntry {
  readonly name: string;
  readonly path: string;
  readonly isDir: boolean;
  readonly bytes?: number;
}

export interface GitEntry {
  readonly path: string;
  readonly status: string;
  readonly staged: boolean;
}

export interface GitStatus {
  readonly isRepo: boolean;
  readonly branch?: string;
  readonly upstream?: string;
  readonly ahead: number;
  readonly behind: number;
  readonly dirty: boolean;
  readonly entries: readonly GitEntry[];
}

const asRecord = (value: unknown): Record<string, unknown> | undefined =>
  typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : undefined;

const str = (value: unknown): string | undefined =>
  typeof value === 'string' && value !== '' ? value : undefined;

const num = (value: unknown): number | undefined => (typeof value === 'number' ? value : undefined);

/** Map one `workspace.tree` level. Rows missing a name/path are dropped rather
 * than rendered as blanks — a malformed row is a bug, not something to show. */
export function toTreeEntries(payload: unknown): TreeEntry[] {
  const raw = asRecord(payload)?.entries;
  if (!Array.isArray(raw)) return [];
  const out: TreeEntry[] = [];
  for (const item of raw) {
    const row = asRecord(item);
    const name = str(row?.name);
    const path = str(row?.path);
    if (name === undefined || path === undefined) continue;
    out.push({ name, path, isDir: row?.kind === 'dir', bytes: num(row?.bytes) });
  }
  return out;
}

/** Map `git.status`. Anything unparseable degrades to "not a repo" so the
 * toolbar disables itself rather than throwing inside a render. */
export function toGitStatus(payload: unknown): GitStatus {
  const row = asRecord(payload);
  const rawEntries = Array.isArray(row?.entries) ? row.entries : [];
  const entries: GitEntry[] = [];
  for (const item of rawEntries) {
    const entry = asRecord(item);
    const path = str(entry?.path);
    if (path === undefined) continue;
    entries.push({ path, status: str(entry?.status) ?? '', staged: entry?.staged === true });
  }
  return {
    isRepo: row?.is_repo === true,
    branch: str(row?.branch),
    upstream: str(row?.upstream),
    ahead: num(row?.ahead) ?? 0,
    behind: num(row?.behind) ?? 0,
    dirty: row?.dirty === true,
    entries,
  };
}

/** Normalize for comparison: forward slashes, lowercase (Windows paths differ
 * in case and separator between the deacon's reply and a tool's own args). */
const normalize = (path: string): string => path.replace(/\\/g, '/').toLowerCase();

/** The top-level folders under `root` that this session actually wrote to,
 * derived from the file paths already carried on its tool rows.
 *
 * Deliberately no new backend state: every write-ish tool row already exposes
 * its `path`, for live events and for a resumed transcript alike, so the
 * session's own history is the source of truth. A session that wrote nothing
 * yields an empty set, and the caller then shows the root unfiltered rather
 * than an empty panel — we don't know its folders, which isn't the same as it
 * having none.
 */
export function sessionFolders(details: readonly string[], root: string): Set<string> {
  const prefix = `${normalize(root).replace(/\/$/, '')}/`;
  const folders = new Set<string>();
  for (const detail of details) {
    const normalized = normalize(detail);
    if (!normalized.startsWith(prefix)) continue;
    const rest = normalized.slice(prefix.length);
    const slash = rest.indexOf('/');
    // A file sitting directly in the root belongs to no folder.
    if (slash <= 0) continue;
    // Return the ORIGINAL casing so the name matches what the tree lists.
    const original = detail.replace(/\\/g, '/').slice(prefix.length);
    folders.add(original.slice(0, slash));
  }
  return folders;
}

// Monaco language ids by extension. Only the common ones — anything else opens
// as plaintext, which still edits and saves fine, just without highlighting.
const LANGUAGES: Record<string, string> = {
  rs: 'rust',
  ts: 'typescript',
  tsx: 'typescript',
  js: 'javascript',
  jsx: 'javascript',
  json: 'json',
  md: 'markdown',
  markdown: 'markdown',
  css: 'css',
  scss: 'scss',
  html: 'html',
  py: 'python',
  go: 'go',
  java: 'java',
  sh: 'shell',
  bash: 'shell',
  yml: 'yaml',
  yaml: 'yaml',
  toml: 'ini',
  sql: 'sql',
  xml: 'xml',
};

export function languageForPath(path: string): string {
  const ext = path.split('.').pop()?.toLowerCase() ?? '';
  // A file with no dot at all (LICENSE, Makefile) reports itself as the ext.
  if (ext === path.toLowerCase()) return 'plaintext';
  return LANGUAGES[ext] ?? 'plaintext';
}
