import { expect, test } from 'bun:test';
import type { TranscriptItem } from '@/shared/kernel/transcript';
import { chatBusy, withTurnError } from '@/features/chat/presentation/chatDisplayState';

test('persistent turn activity keeps chat busy after local state remounts', () => {
  expect(chatBusy(false, 'running')).toBe(true);
  expect(chatBusy(true, 'idle')).toBe(true);
  expect(chatBusy(false, 'done')).toBe(false);
});

test('a missed turn error is appended once', () => {
  const items: TranscriptItem[] = [{ kind: 'user', text: 'proceed' }];
  const once = withTurnError(items, 'provider failure: network error');
  const twice = withTurnError(once, 'provider failure: network error');

  expect(once).toEqual([
    ...items,
    { kind: 'error', message: 'provider failure: network error' },
  ]);
  expect(twice).toBe(once);
});

test('no bus error leaves transcript items untouched', () => {
  const items: TranscriptItem[] = [{ kind: 'user', text: 'hello' }];
  expect(withTurnError(items, undefined)).toBe(items);
});
