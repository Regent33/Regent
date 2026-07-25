'use client';
// What the coding panel currently shows — published by the panel, read by the
// chat when it submits a turn so the agent knows what the user is looking at.
// Same tiny-store shape as activeSession.ts.
//
// Two levels, mirroring how editors report to coding agents generally:
//   * the open FILE travels as a path only — the agent has file tools and the
//     file is inside its own workspace, so a path points it at the right place
//     for zero prompt tokens;
//   * a SELECTION travels with its text, because "this bit here" is precisely
//     what the agent cannot infer from a path.
import { type Store, createStore, useStore } from '@/shared/state/store';

/** Selected text is capped: a whole-file select-all would otherwise paste the
 * file into every following turn. The line range still tells the agent exactly
 * what was highlighted, and it can read the rest itself. */
export const SELECTION_MAX_CHARS = 2000;

export interface EditorSelection {
  readonly startLine: number;
  readonly endLine: number;
  readonly text: string;
}

export interface OpenFileState {
  readonly path: string | undefined;
  readonly selection: EditorSelection | undefined;
}

const store: Store<OpenFileState> = createStore<OpenFileState>({
  path: undefined,
  selection: undefined,
});

export function setOpenFile(path: string | undefined): void {
  // Changing file drops any selection — it belonged to the previous document.
  store.setState({ path, selection: undefined });
}

export function setOpenSelection(selection: EditorSelection | undefined): void {
  store.setState((prev) => (prev.path === undefined ? prev : { ...prev, selection }));
}

/** Read outside React (the submit path) — the chat must not re-render on every
 * cursor move in the editor. */
export function currentOpenFile(): OpenFileState {
  return store.getState();
}

export function useOpenFilePath(): string | undefined {
  return useStore(store, (s) => s.path);
}
