import { expect, test } from 'bun:test';
import { menuForTarget } from '@/shared/ui/contextMenu/menuForTarget';

test('read-only text with a selection offers only Copy', () => {
  const items = menuForTarget({ editable: false, hasSelection: true, canPaste: true });
  expect(items.map((i) => i.id)).toEqual(['copy', 'selectAll']);
});

test('an editable field with a selection offers Cut/Copy/Paste/Select all', () => {
  const items = menuForTarget({ editable: true, hasSelection: true, canPaste: true });
  expect(items.map((i) => i.id)).toEqual(['cut', 'copy', 'paste', 'selectAll']);
});

test('Cut and Copy are disabled without a selection', () => {
  const items = menuForTarget({ editable: true, hasSelection: false, canPaste: true });
  const byId = Object.fromEntries(items.map((i) => [i.id, i]));
  expect(byId.cut?.enabled).toBe(false);
  expect(byId.copy?.enabled).toBe(false);
  expect(byId.paste?.enabled).toBe(true);
});

test('Paste is omitted when the clipboard cannot be read at all', () => {
  const items = menuForTarget({ editable: true, hasSelection: false, canPaste: false });
  expect(items.some((i) => i.id === 'paste')).toBe(false);
});

test('plain non-editable area with no selection offers only Select all', () => {
  const items = menuForTarget({ editable: false, hasSelection: false, canPaste: true });
  expect(items.map((i) => i.id)).toEqual(['selectAll']);
});
