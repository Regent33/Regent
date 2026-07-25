// A prompt typed while a turn is busy is queued (not dropped) and flushed
// FIFO once the phase returns to idle — so the user can keep typing mid-think
// instead of the composer silently no-opping. Pure state + two operations;
// the view owns the mutable instance and the effect that drains it.
import type { ChatPhase } from "./transcript.ts";

export interface PromptQueue {
  readonly items: string[];
}

export function createPromptQueue(): PromptQueue {
  return { items: [] };
}

/** Queue `text` if `phase` is busy; returns its 1-based queue position, or
 * `undefined` when not queued (the caller should send it immediately). */
export function enqueueIfBusy(
  queue: PromptQueue,
  phase: ChatPhase,
  text: string,
): number | undefined {
  if (phase !== "busy") return undefined;
  queue.items.push(text);
  return queue.items.length;
}

/** Pop the next queued prompt once `phase` is idle; `undefined` if the phase
 * isn't idle yet or nothing is queued. */
export function dequeueOnIdle(queue: PromptQueue, phase: ChatPhase): string | undefined {
  if (phase !== "idle" || queue.items.length === 0) return undefined;
  return queue.items.shift();
}
