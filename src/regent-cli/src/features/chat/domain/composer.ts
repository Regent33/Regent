// The composer's editing model, as pure functions over {value, pos}.
//
// It lives outside the component for two reasons: the key bindings and the help
// overlay have to come from ONE table or they drift, and cursor/paste logic is
// exactly the sort of thing that regresses silently — pure functions can be
// tested without a terminal, a PTY, or a rendering library.

/** Terminal escape sequences that turn bracketed paste on and off. */
export const PASTE_ON = "\u001b[?2004h";
export const PASTE_OFF = "\u001b[?2004l";
// Ink strips the leading ESC from an unrecognised sequence before the handler
// sees it, so the markers arrive WITHOUT it — which is why what we write and
// what we match are not the same strings. Verified against ink 7.1.1.
const PASTE_START = "[200~";
const PASTE_END = "[201~";

export interface Composer {
  readonly value: string;
  readonly pos: number;
  /** Inside a bracketed paste: newlines are content, never submit. */
  readonly pasting: boolean;
}

export const EMPTY: Composer = { value: "", pos: 0, pasting: false };

export function at(value: string, pos: number): Composer {
  return { value, pos: Math.max(0, Math.min(pos, value.length)), pasting: false };
}

/** Insert `text` at the cursor. */
function insert(s: Composer, text: string): Composer {
  return {
    ...s,
    value: s.value.slice(0, s.pos) + text + s.value.slice(s.pos),
    pos: s.pos + text.length,
  };
}

/** Cut the region between `from` and `to`, leaving the cursor at the lower end. */
function cut(s: Composer, from: number, to: number): Composer {
  const [a, b] = from <= to ? [from, to] : [to, from];
  return { ...s, value: s.value.slice(0, a) + s.value.slice(b), pos: a };
}

const lineStart = (v: string, pos: number) => v.lastIndexOf("\n", pos - 1) + 1;
const lineEnd = (v: string, pos: number) => {
  const i = v.indexOf("\n", pos);
  return i === -1 ? v.length : i;
};

/** Start of the word before `pos`: skip trailing spaces, then the word itself. */
function wordLeft(v: string, pos: number): number {
  let i = pos;
  while (i > 0 && /\s/.test(v[i - 1] as string)) i--;
  while (i > 0 && !/\s/.test(v[i - 1] as string)) i--;
  return i;
}

function wordRight(v: string, pos: number): number {
  let i = pos;
  while (i < v.length && /\s/.test(v[i] as string)) i++;
  while (i < v.length && !/\s/.test(v[i] as string)) i++;
  return i;
}

/** Move the cursor one visual row, keeping the column where it can. */
function verticalMove(s: Composer, delta: -1 | 1): Composer | null {
  const start = lineStart(s.value, s.pos);
  const column = s.pos - start;
  if (delta === -1) {
    if (start === 0) return null; // no line above — caller falls back to history
    const prevStart = lineStart(s.value, start - 1);
    return { ...s, pos: Math.min(prevStart + column, start - 1) };
  }
  const end = lineEnd(s.value, s.pos);
  if (end === s.value.length) return null; // no line below
  const nextEnd = lineEnd(s.value, end + 1);
  return { ...s, pos: Math.min(end + 1 + column, nextEnd) };
}

/** The subset of ink's key flags the composer reads. */
export interface KeyFlags {
  readonly return?: boolean;
  readonly leftArrow?: boolean;
  readonly rightArrow?: boolean;
  readonly upArrow?: boolean;
  readonly downArrow?: boolean;
  readonly home?: boolean;
  readonly end?: boolean;
  readonly backspace?: boolean;
  readonly delete?: boolean;
  readonly ctrl?: boolean;
  readonly meta?: boolean;
  readonly shift?: boolean;
  readonly escape?: boolean;
  readonly tab?: boolean;
}

/** "submit" = send it; null = the key means nothing here (caller may handle it). */
export type Edit = Composer | "submit" | null;

/**
 * Apply one key press. Paste handling comes first: between the bracketed-paste
 * markers every byte is content, so a stack trace pasted into the prompt stays
 * one message instead of the first newline sending it and the rest scattering
 * across the next turns.
 */
export function applyKey(s: Composer, input: string, key: KeyFlags): Edit {
  if (input.includes(PASTE_START) || s.pasting) {
    return applyPaste(s, input);
  }

  if (key.return) {
    // alt+enter and shift+enter both insert a newline where the terminal sends
    // them; plain enter always submits, so the muscle memory never changes.
    if (key.meta || key.shift) return insert(s, "\n");
    return "submit";
  }
  if (key.ctrl) {
    switch (input) {
      case "a":
        return { ...s, pos: lineStart(s.value, s.pos) };
      case "e":
        return { ...s, pos: lineEnd(s.value, s.pos) };
      case "w":
        return cut(s, wordLeft(s.value, s.pos), s.pos);
      case "u":
        return cut(s, lineStart(s.value, s.pos), s.pos);
      case "k":
        return cut(s, s.pos, lineEnd(s.value, s.pos));
      default:
        break;
    }
  }
  if (key.home) return { ...s, pos: lineStart(s.value, s.pos) };
  if (key.end) return { ...s, pos: lineEnd(s.value, s.pos) };
  if (key.leftArrow) {
    return { ...s, pos: key.ctrl ? wordLeft(s.value, s.pos) : Math.max(0, s.pos - 1) };
  }
  if (key.rightArrow) {
    return {
      ...s,
      pos: key.ctrl ? wordRight(s.value, s.pos) : Math.min(s.value.length, s.pos + 1),
    };
  }
  // Only move between lines when there ARE lines; otherwise up/down stay the
  // history keys they have always been.
  if (key.upArrow) return verticalMove(s, -1);
  if (key.downArrow) return verticalMove(s, 1);
  // Terminals disagree on whether Backspace reports as `backspace` or `delete`,
  // so both delete backwards (otherwise Backspace is a no-op at end of line).
  if (key.backspace || key.delete) {
    return s.pos > 0 ? cut(s, s.pos - 1, s.pos) : s;
  }
  if (input && !key.ctrl && !key.meta && !key.escape) return insert(s, input);
  return null;
}

/** Consume a chunk that is part of a bracketed paste. */
function applyPaste(s: Composer, input: string): Composer {
  let text = input;
  let pasting = s.pasting;
  const start = text.indexOf(PASTE_START);
  if (start !== -1) {
    text = text.slice(start + PASTE_START.length);
    pasting = true;
  }
  const end = text.indexOf(PASTE_END);
  if (end !== -1) {
    text = text.slice(0, end);
    pasting = false;
  }
  // \r\n and a bare \r both mean "new line" inside pasted text.
  const normalised = text.replace(/\r\n?/g, "\n");
  return { ...insert(s, normalised), pasting };
}

/** The bindings, in the order the overlay shows them. One table, two consumers. */
export const BINDINGS: ReadonlyArray<readonly [string, string]> = [
  ["enter", "send"],
  ["alt/shift+enter", "new line"],
  ["↑ ↓", "move a line · recall history when single-line"],
  ["← →", "move by character"],
  ["ctrl+← →", "move by word"],
  ["home / ctrl+a", "start of line"],
  ["end / ctrl+e", "end of line"],
  ["ctrl+w", "delete the word before the cursor"],
  ["ctrl+u", "delete to start of line"],
  ["ctrl+k", "delete to end of line"],
  ["/", "command picker — ↑↓ select · ⇥ complete · ↵ run"],
  ["ctrl+c", "interrupt · twice to quit"],
];
