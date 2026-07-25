'use client';
// Blocks the browser/webview devtools-access shortcuts so a packaged Regent
// window doesn't read as "just a browser tab". F5/Ctrl+R (reload) are
// deliberately left alone — this only targets devtools/view-source access.
import { useEffect } from 'react';

const BLOCKED_LETTERS = new Set(['i', 'j', 'c']);

/** Structural, not the DOM type — a pure predicate over plain key-event
 * fields, testable without a DOM/jsdom environment. */
export interface KeyChord {
  readonly key: string;
  readonly ctrlKey: boolean;
  readonly metaKey: boolean;
  readonly shiftKey: boolean;
  readonly altKey: boolean;
}

export function isDevtoolsShortcut(e: KeyChord): boolean {
  if (e.key === 'F12') return true;
  const key = e.key.toLowerCase();
  const primary = e.ctrlKey || e.metaKey;
  if (primary && !e.shiftKey && !e.altKey && key === 'u') return true; // view-source
  // DevTools panes: Ctrl+Shift+I/J/C on Windows/Linux, Cmd+Opt+I/J/C on macOS.
  if (primary && (e.shiftKey || e.altKey) && BLOCKED_LETTERS.has(key)) return true;
  return false;
}

export function useBlockDevtoolsKeys(): void {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isDevtoolsShortcut(e)) e.preventDefault();
    };
    window.addEventListener('keydown', onKey, { capture: true });
    return () => window.removeEventListener('keydown', onKey, { capture: true });
  }, []);
}
