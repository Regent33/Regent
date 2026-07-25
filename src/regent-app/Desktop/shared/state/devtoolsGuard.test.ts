import { expect, test } from 'bun:test';
import { isDevtoolsShortcut, type KeyChord } from '@/shared/state/devtoolsGuard';

function key(init: { key: string } & Partial<Omit<KeyChord, 'key'>>): KeyChord {
  return { ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, ...init };
}

test('F12 opens devtools and is blocked', () => {
  expect(isDevtoolsShortcut(key({ key: 'F12' }))).toBe(true);
});

test('Ctrl+Shift+I/J/C open devtools panes and are blocked', () => {
  expect(isDevtoolsShortcut(key({ key: 'I', ctrlKey: true, shiftKey: true }))).toBe(true);
  expect(isDevtoolsShortcut(key({ key: 'J', ctrlKey: true, shiftKey: true }))).toBe(true);
  expect(isDevtoolsShortcut(key({ key: 'C', ctrlKey: true, shiftKey: true }))).toBe(true);
});

test('Ctrl+U (view source) is blocked', () => {
  expect(isDevtoolsShortcut(key({ key: 'U', ctrlKey: true }))).toBe(true);
});

test('F5 and Ctrl+R (reload) are explicitly left alone', () => {
  expect(isDevtoolsShortcut(key({ key: 'F5' }))).toBe(false);
  expect(isDevtoolsShortcut(key({ key: 'R', ctrlKey: true }))).toBe(false);
});

test('plain typing and unrelated shortcuts are left alone', () => {
  expect(isDevtoolsShortcut(key({ key: 'i' }))).toBe(false);
  expect(isDevtoolsShortcut(key({ key: 'c', ctrlKey: true }))).toBe(false); // plain Ctrl+C copy
  expect(isDevtoolsShortcut(key({ key: 'Enter' }))).toBe(false);
});

test('the Mac Cmd equivalents are also blocked', () => {
  expect(isDevtoolsShortcut(key({ key: 'I', metaKey: true, altKey: true }))).toBe(true);
});
