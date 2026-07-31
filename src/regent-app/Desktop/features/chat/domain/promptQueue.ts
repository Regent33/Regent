// A submit while a turn is busy is queued (not silently dropped) and flushed
// FIFO once the turn ends — mirrors the CLI's promptQueue.ts (a parallel
// file, not shared: this repo doesn't share code between the two front-ends,
// same as the voice locate/spawn logic being ported separately to each).

export interface QueuedPrompt {
  readonly text: string;
  readonly attachments?: readonly File[];
}

export interface PromptQueue {
  readonly items: QueuedPrompt[];
}

export function createPromptQueue(): PromptQueue {
  return { items: [] };
}

/** What a submit made while a turn is running should do. */
export type BusySubmit = 'send' | 'barge' | 'queue';

/** Sending mid-turn INTERRUPTS by default — the same gesture Butler has, and
 * what people mean when they type over an answer they no longer want. Queueing
 * is still there for a follow-up thought you want handled after this one, on
 * Ctrl/Cmd+Enter.
 *
 * This started the other way round, with Enter queueing and Ctrl+Enter barging,
 * and it was wrong: pressing Enter to interrupt is the reflex, and a modifier
 * nobody has been told about is not a feature. */
export function busySubmitAction(busy: boolean, modifier: boolean): BusySubmit {
  if (!busy) return 'send';
  return modifier ? 'queue' : 'barge';
}

/** Queue `prompt` if `busy`; returns its 1-based queue position, or
 * `undefined` when not queued (the caller should send it immediately). */
export function enqueueIfBusy(queue: PromptQueue, busy: boolean, prompt: QueuedPrompt): number | undefined {
  if (!busy) return undefined;
  queue.items.push(prompt);
  return queue.items.length;
}

/** Pop the next queued prompt once the turn is no longer busy; `undefined`
 * if still busy or nothing is queued. */
export function dequeueOnBusyEnd(queue: PromptQueue, busy: boolean): QueuedPrompt | undefined {
  if (busy || queue.items.length === 0) return undefined;
  return queue.items.shift();
}
