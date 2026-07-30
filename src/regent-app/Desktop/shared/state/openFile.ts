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
  /** A folder highlighted in the tree — worth telling the agent about even
   * when no file is open ("work in this directory"). */
  readonly folder: string | undefined;
  /** User's switch. Off means NOTHING from the panel travels with a turn —
   * some conversations simply aren't about the file that happens to be open,
   * and silently attaching it would steer the answer. */
  readonly enabled: boolean;
}

const store: Store<OpenFileState> = createStore<OpenFileState>({
  path: undefined,
  selection: undefined,
  folder: undefined,
  enabled: true,
});

export function setOpenFile(path: string | undefined): void {
  // Changing file drops any selection — it belonged to the previous document.
  store.setState((prev) => ({ ...prev, path, selection: undefined }));
}

export function setSelectedFolder(folder: string | undefined): void {
  store.setState((prev) => ({ ...prev, folder }));
}

export function setContextEnabled(enabled: boolean): void {
  store.setState((prev) => ({ ...prev, enabled }));
}

/** Forget what the panel was showing entirely — file, selection and folder.
 *
 * This store is module-global while the panel that fills it is conditionally
 * mounted, so a tree selection outlived the panel AND the conversation: closing
 * the panel and starting a fresh chat still showed a chip reading `.agents`,
 * with nothing on screen to explain where it came from. Reported 2026-07-30.
 *
 * `enabled` deliberately survives. It is the user's own switch, and clearing a
 * stale path is not a reason to re-arm a switch they turned off. */
export function clearEditorContext(): void {
  store.setState((prev) => ({
    ...prev,
    path: undefined,
    selection: undefined,
    folder: undefined,
  }));
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

/** What the composer shows in its context chip, and the switch that governs it. */
export function useEditorContext(): {
  path: string | undefined;
  folder: string | undefined;
  hasSelection: boolean;
  enabled: boolean;
} {
  const path = useStore(store, (s) => s.path);
  const folder = useStore(store, (s) => s.folder);
  const hasSelection = useStore(store, (s) => s.selection !== undefined);
  const enabled = useStore(store, (s) => s.enabled);
  return { path, folder, hasSelection, enabled };
}
