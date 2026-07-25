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
