// Session-local composer input history (cap 50) — ↑/↓ cycle through
// previously submitted prompts when the composer is empty or holds an
// unedited history entry. Plain refs (no re-render needed): callers read the
// returned string synchronously from a keydown handler. Dies with the
// component (new Composer instance per session, per the remount-per-session
// architecture), so history never leaks across sessions.
import { useRef } from 'react';

const CAP = 50;

export interface InputHistory {
  /** Record a just-submitted prompt and reset browsing. */
  record: (text: string) => void;
  /** ↑ — returns the next-older entry, or undefined if there is none / the
   * composer isn't in an "empty or unedited" state. */
  up: (current: string) => string | undefined;
  /** ↓ — returns the next-newer entry (or the pre-browse draft once back past
   * the newest), or undefined if not currently browsing. */
  down: (current: string) => string | undefined;
}

export function useInputHistory(): InputHistory {
  const entriesRef = useRef<string[]>([]);
  const cursorRef = useRef(-1); // -1 = not browsing; 0 = newest entry
  const draftRef = useRef('');

  const record = (text: string) => {
    entriesRef.current = [text, ...entriesRef.current].slice(0, CAP);
    cursorRef.current = -1;
    draftRef.current = '';
  };

  const up = (current: string): string | undefined => {
    const entries = entriesRef.current;
    if (entries.length === 0) return undefined;
    const cursor = cursorRef.current;
    const editable = cursor === -1 ? current.trim() === '' : current === entries[cursor];
    if (!editable) return undefined;
    if (cursor === -1) draftRef.current = current;
    if (cursor >= entries.length - 1) return undefined;
    cursorRef.current = cursor + 1;
    return entries[cursorRef.current];
  };

  const down = (current: string): string | undefined => {
    const cursor = cursorRef.current;
    if (cursor === -1 || current !== entriesRef.current[cursor]) return undefined;
    cursorRef.current = cursor - 1;
    if (cursorRef.current === -1) {
      const draft = draftRef.current;
      draftRef.current = '';
      return draft;
    }
    return entriesRef.current[cursorRef.current];
  };

  return { record, up, down };
}
