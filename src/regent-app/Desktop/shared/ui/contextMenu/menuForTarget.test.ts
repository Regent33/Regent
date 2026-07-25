import { expect, test } from 'bun:test';
import { menuForTarget } from '@/shared/ui/contextMenu/menuForTarget';

// The menu always shows the same four items, enabling only what applies. The
// first version omitted inapplicable ones, which made a right-click on chat
// text show a lone "Select all" and read as broken.
const ids = ['cut', 'copy', 'paste', 'selectAll'];

test('read-only text with a selection can copy but not cut or paste into it', () => {
  const items = menuForTarget({ editable: false, hasSelection: true, canPaste: true });
  expect(items.map((i) => i.id)).toEqual(ids);
  const byId = Object.fromEntries(items.map((i) => [i.id, i]));
  expect(byId.copy?.enabled).toBe(true);
  expect(byId.cut?.enabled).toBe(false);
  expect(byId.paste?.enabled).toBe(false);
});

test('an editable field with a selection enables everything', () => {
  const items = menuForTarget({ editable: true, hasSelection: true, canPaste: true });
  expect(items.map((i) => i.id)).toEqual(ids);
  expect(items.every((i) => i.enabled)).toBe(true);
});

test('Cut and Copy are disabled without a selection, Paste stays available', () => {
  const items = menuForTarget({ editable: true, hasSelection: false, canPaste: true });
  const byId = Object.fromEntries(items.map((i) => [i.id, i]));
  expect(byId.cut?.enabled).toBe(false);
  expect(byId.copy?.enabled).toBe(false);
  expect(byId.paste?.enabled).toBe(true);
});

test('Paste is shown but disabled when the clipboard cannot be read', () => {
  const items = menuForTarget({ editable: true, hasSelection: false, canPaste: false });
  const paste = items.find((i) => i.id === 'paste');
  expect(paste === undefined).toBe(false);
  expect(paste?.enabled).toBe(false);
});

test('a plain area with no selection still shows the full menu, mostly disabled', () => {
  const items = menuForTarget({ editable: false, hasSelection: false, canPaste: true });
  expect(items.map((i) => i.id)).toEqual(ids);
  const byId = Object.fromEntries(items.map((i) => [i.id, i]));
  expect(byId.selectAll?.enabled).toBe(true);
  expect(byId.copy?.enabled).toBe(false);
});
