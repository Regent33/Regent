import { expect, test } from 'bun:test';
import {
  bargeNotice,
  busySubmitAction,
  createPromptQueue,
  dequeueOnBusyEnd,
  enqueueIfBusy,
} from '@/features/chat/domain/promptQueue';
import { t } from '@/shared/i18n/t';

// Reported: sending mid-turn sat in the queue instead of interrupting. It was
// built the other way round — Enter queued, Ctrl+Enter barged — and a modifier
// nobody has been told about is not a feature.
test('sending mid-turn interrupts; the modifier is what queues', () => {
  expect(busySubmitAction(true, false)).toBe('barge');
  expect(busySubmitAction(true, true)).toBe('queue');
});

test('an idle chat just sends, modifier or not', () => {
  expect(busySubmitAction(false, false)).toBe('send');
  expect(busySubmitAction(false, true)).toBe('send');
});

// Asked for: more than one acknowledgement, so repeated barge-ins don't echo
// the same sentence back. Rotation, not randomness — two in a row never match.
test('the barge acknowledgement rotates and wraps', () => {
  const lines = t().chat.composer.interrupted;
  expect(lines.length).toBeGreaterThan(1);
  expect(bargeNotice(lines, 0)).toBe(lines[0]);
  expect(bargeNotice(lines, 1)).toBe(lines[1]);
  expect(bargeNotice(lines, lines.length)).toBe(lines[0]); // wraps
  expect(bargeNotice(lines, -1)).toBe(lines[lines.length - 1]); // never crashes
});

test('a submit while busy is queued, not sent — position is 1-based', () => {
  const queue = createPromptQueue();
  expect(enqueueIfBusy(queue, true, { text: 'first' })).toBe(1);
  expect(enqueueIfBusy(queue, true, { text: 'second' })).toBe(2);
});

test('a submit while idle is never queued', () => {
  const queue = createPromptQueue();
  expect(enqueueIfBusy(queue, false, { text: 'go' })).toBeUndefined();
  expect(queue.items).toEqual([]);
});

test('attachments travel with the queued item', () => {
  const queue = createPromptQueue();
  const file = new File(['x'], 'a.txt');
  enqueueIfBusy(queue, true, { text: 'with a file', attachments: [file] });
  expect(dequeueOnBusyEnd(queue, false)?.attachments).toEqual([file]);
});

test('dequeue only fires once busy ends, FIFO, and only when something is queued', () => {
  const queue = createPromptQueue();
  enqueueIfBusy(queue, true, { text: 'first' });
  enqueueIfBusy(queue, true, { text: 'second' });

  expect(dequeueOnBusyEnd(queue, true)).toBeUndefined(); // still busy — not due yet
  expect(dequeueOnBusyEnd(queue, false)?.text).toBe('first');
  expect(dequeueOnBusyEnd(queue, false)?.text).toBe('second');
  expect(dequeueOnBusyEnd(queue, false)).toBeUndefined(); // drained
});
