// The deacon DECORATES a submitted prompt before running it — one
// `[attached file: <path>]` line per staged attachment, then a bracketed
// `[The user …]` note describing what the coding panel had open — and stores
// the decorated text. Replaying a session therefore replays that plumbing as
// prose inside the user's bubble, which is not what they typed.
//
// Split it back out here so every surface shows the words the user wrote and
// renders the rest as chips above the message. One parser, so a live message
// and a replayed one can never disagree.

export interface DecoratedPrompt {
  /** The user's own words. */
  readonly text: string;
  /** File names (not full paths — the staging directory is noise here). */
  readonly attachments: readonly string[];
  /** Short label for what the editor had open, when the turn carried one. */
  readonly context?: string;
}

const ATTACHMENT = /^\[attached file: (.+)\]$/;
/** Where the editor note begins. The deacon always appends it last, always
 * with this exact prefix (see `dispatcher::editor_context`). */
const EDITOR_NOTE = '\n\n[The user ';

const FOLDER = /^\[The user has the (.+) folder selected\.\]$/;
const OPEN_FILE = /^\[The user has (.+) open in the editor\.\]$/;
const SELECTION = /^\[The user is looking at (.+) in the editor and has lines (\d+-\d+) selected:/;

/** Last path segment of a path (either slash flavor). */
const baseName = (path: string): string => path.split(/[/\\]/).at(-1) ?? path;

/** The note → a chip label, or undefined if it isn't a shape we know. */
function contextLabel(note: string): string | undefined {
  const trimmed = note.trim();
  const selection = SELECTION.exec(trimmed);
  if (selection !== null) return `${baseName(selection[1])} · ${selection[2]}`;
  const open = OPEN_FILE.exec(trimmed);
  if (open !== null) return baseName(open[1]);
  const folder = FOLDER.exec(trimmed);
  if (folder !== null) return baseName(folder[1]);
  return undefined;
}

/** The same chip label, built from what the editor holds RIGHT NOW rather than
 * from a stored note — so the chip on a message you just sent matches the one
 * on that message after a reload. */
export function editorChipLabel(open: {
  readonly path?: string;
  readonly folder?: string;
  readonly selection?: { readonly startLine: number; readonly endLine: number };
}): string | undefined {
  if (open.path !== undefined) {
    const name = baseName(open.path);
    return open.selection === undefined
      ? name
      : `${name} · ${open.selection.startLine}-${open.selection.endLine}`;
  }
  return open.folder === undefined ? undefined : baseName(open.folder);
}

export function splitPromptDecorations(raw: string): DecoratedPrompt {
  // The editor note goes on last and can span lines (a selection carries its
  // text), so cut it as a block before the per-line attachment pass.
  let body = raw;
  let context: string | undefined;
  const noteAt = body.indexOf(EDITOR_NOTE);
  if (noteAt !== -1 && body.trimEnd().endsWith(']')) {
    context = contextLabel(body.slice(noteAt + 2));
    // Only cut when it parsed as a note we recognise — otherwise it is prose
    // that happens to start that way, and eating it would lose the message.
    if (context !== undefined) body = body.slice(0, noteAt);
  }

  const attachments: string[] = [];
  const kept = body.split('\n').filter((line) => {
    const match = ATTACHMENT.exec(line.trim());
    if (match === null) return true;
    attachments.push(baseName(match[1].trim()));
    return false;
  });
  return { text: kept.join('\n').trim(), attachments, ...(context === undefined ? {} : { context }) };
}
