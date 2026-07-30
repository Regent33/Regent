// What the TERMINAL handles versus what the shell gets.
//
// The hard case is Ctrl+C, which means two different things. With a selection it
// is copy; with none it is SIGINT, and a terminal that always copied would take
// away the only way to stop a runaway process. VS Code resolves it exactly this
// way, and so does this.
//
// Pure and structural (no DOM types) so every rule is testable — the repo has no
// jsdom, and "which key does what" is precisely the part worth pinning.

/** The subset of KeyboardEvent this needs. */
export interface TerminalChord {
  readonly key: string;
  readonly ctrlKey: boolean;
  readonly metaKey: boolean;
  readonly shiftKey: boolean;
  readonly altKey: boolean;
  readonly type: string;
}

export type TerminalAction =
  | 'copy'
  | 'paste'
  | 'selectAll'
  | 'search'
  | 'clear'
  | 'newTerminal'
  | 'closeTerminal'
  | 'shell';

/** What a chord should do. `'shell'` means "hand it to the process untouched",
 * which is the default and must stay the default — every intercepted chord is
 * one a program running in the terminal can no longer see. */
export function terminalAction(chord: TerminalChord, hasSelection: boolean): TerminalAction {
  if (chord.type !== 'keydown') return 'shell';
  const mod = chord.ctrlKey || chord.metaKey;
  if (!mod) return 'shell';
  const key = chord.key.toLowerCase();

  // Explicit copy/paste — unambiguous, and the chord people reach for when
  // Ctrl+C is busy being SIGINT.
  if (chord.shiftKey) {
    if (key === 'c') return 'copy';
    if (key === 'v') return 'paste';
    if (key === 'f') return 'search';
    // Ctrl+Shift+5 is VS Code's split; a new terminal is the nearest honest
    // equivalent here, since this panel does not split.
    if (key === '5') return 'newTerminal';
    return 'shell';
  }

  switch (key) {
    // The two-meanings case. Copy only when there is something to copy;
    // otherwise SIGINT reaches the process, which is what Ctrl+C is FOR.
    case 'c':
      return hasSelection ? 'copy' : 'shell';
    // On macOS Cmd+V has no shell meaning, so it is always paste. On Windows and
    // Linux Ctrl+V is a legitimate control byte (0x16, "quoted insert" in
    // readline), but pasting is what people press it for a thousand times more
    // often — and Ctrl+Shift+V is still there for the rare other case.
    case 'v':
      return 'paste';
    case 'f':
      return 'search';
    case 'a':
      // Only with Cmd (macOS select-all). Ctrl+A is start-of-line in every shell
      // on earth and stealing it would be unforgivable.
      return chord.metaKey && !chord.ctrlKey ? 'selectAll' : 'shell';
    case 'k':
      // Cmd+K clears on macOS. Ctrl+K is kill-to-end-of-line in readline, so it
      // stays with the shell.
      return chord.metaKey && !chord.ctrlKey ? 'clear' : 'shell';
    default:
      return 'shell';
  }
}

/** Ctrl/Cmd+click is "follow this", the same gesture as in the editor. Plain
 * clicks must stay plain — a terminal is also a place you select text. */
export function isFollowClick(chord: { ctrlKey: boolean; metaKey: boolean }): boolean {
  return chord.ctrlKey || chord.metaKey;
}
