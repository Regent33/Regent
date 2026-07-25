import { expect, test } from 'bun:test';
import { createPromptQueue, dequeueOnBusyEnd, enqueueIfBusy } from '@/features/chat/domain/promptQueue';

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
