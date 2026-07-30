import { expect, test } from 'bun:test';
import {
  clearEditorContext,
  currentOpenFile,
  setContextEnabled,
  setOpenFile,
  setOpenSelection,
  setSelectedFolder,
} from '@/shared/state/openFile';

// Reported 2026-07-30: a folder picked in the tree stayed on the composer chip
// after the panel closed and after a NEW conversation was started, and the chip
// had no way to dismiss it — only a toggle that struck it through.
test('clearing forgets file, selection and folder but keeps the user switch', () => {
  setOpenFile('d:/repo/.agents/main.rs');
  setOpenSelection({ startLine: 1, endLine: 2, text: 'fn main() {}' });
  setSelectedFolder('d:/repo/.agents');
  // Off means the user already said "don't share this".
  setContextEnabled(false);

  clearEditorContext();

  const after = currentOpenFile();
  expect(after.path).toBeUndefined();
  expect(after.selection).toBeUndefined();
  expect(after.folder).toBeUndefined();
  // The switch is theirs — dropping a stale path must not silently re-arm it.
  expect(after.enabled).toBe(false);

  setContextEnabled(true); // leave the module-global store as it was found
});
