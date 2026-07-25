import { expect, test } from 'bun:test';
import { diffLines } from '@/shared/ui/diffLines';

test('identical text is all unchanged', () => {
  expect(diffLines('a\nb\nc', 'a\nb\nc')).toEqual([
    { kind: 'unchanged', text: 'a' },
    { kind: 'unchanged', text: 'b' },
    { kind: 'unchanged', text: 'c' },
  ]);
});

test('a replaced middle line reports one removed and one added, context kept', () => {
  expect(diffLines('a\nb\nc', 'a\nB\nc')).toEqual([
    { kind: 'unchanged', text: 'a' },
    { kind: 'removed', text: 'b' },
    { kind: 'added', text: 'B' },
    { kind: 'unchanged', text: 'c' },
  ]);
});

test('an appended line is added only, with prior lines unchanged', () => {
  expect(diffLines('a\nb', 'a\nb\nc')).toEqual([
    { kind: 'unchanged', text: 'a' },
    { kind: 'unchanged', text: 'b' },
    { kind: 'added', text: 'c' },
  ]);
});

test('a deleted line is removed only, with remaining lines unchanged', () => {
  expect(diffLines('a\nb\nc', 'a\nc')).toEqual([
    { kind: 'unchanged', text: 'a' },
    { kind: 'removed', text: 'b' },
    { kind: 'unchanged', text: 'c' },
  ]);
});

test('a wholesale rewrite reports every old line removed and every new line added', () => {
  expect(diffLines('x\ny', 'p\nq')).toEqual([
    { kind: 'removed', text: 'x' },
    { kind: 'removed', text: 'y' },
    { kind: 'added', text: 'p' },
    { kind: 'added', text: 'q' },
  ]);
});

test('an empty before is a pure addition', () => {
  expect(diffLines('', 'a\nb')).toEqual([
    { kind: 'removed', text: '' },
    { kind: 'added', text: 'a' },
    { kind: 'added', text: 'b' },
  ]);
});
