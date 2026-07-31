import { expect, test } from 'bun:test';
import { rowToItems } from '@/features/chat/data/eventDetails';

test('a stored assistant reply replays as an assistant bubble', () => {
  expect(rowToItems({ role: 'assistant', text: 'here it is' })).toEqual([
    { kind: 'assistant', text: 'here it is', streaming: false },
  ]);
});

// The backend keeps the interrupted QUESTION in context by closing the
// exchange with a placeholder reply. Replaying that as a normal bubble made
// Regent look like it had answered "(no reply — …)"; it is bookkeeping.
test('the stopped-turn placeholder replays as a quiet notice, not a reply', () => {
  const text = '(no reply — the user interrupted me before I answered)';
  expect(rowToItems({ role: 'assistant', text })).toEqual([
    { kind: 'notice', text, tone: 'ok' },
  ]);
});
