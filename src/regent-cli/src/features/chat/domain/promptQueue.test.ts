import { expect, test } from "bun:test";
import { createPromptQueue, dequeueOnIdle, enqueueIfBusy } from "./promptQueue.ts";

test("a prompt submitted while busy is queued, not sent — position is 1-based", () => {
  const queue = createPromptQueue();
  expect(enqueueIfBusy(queue, "busy", "first")).toBe(1);
  expect(enqueueIfBusy(queue, "busy", "second")).toBe(2);
});

test("a prompt submitted while idle or approving is never queued", () => {
  const queue = createPromptQueue();
  expect(enqueueIfBusy(queue, "idle", "go")).toBeUndefined();
  expect(enqueueIfBusy(queue, "approving", "go")).toBeUndefined();
  expect(queue.items).toEqual([]);
});

test("dequeue only fires once idle, FIFO, and only when something is queued", () => {
  const queue = createPromptQueue();
  enqueueIfBusy(queue, "busy", "first");
  enqueueIfBusy(queue, "busy", "second");

  expect(dequeueOnIdle(queue, "busy")).toBeUndefined(); // still busy — not due yet
  expect(dequeueOnIdle(queue, "idle")).toBe("first");
  expect(dequeueOnIdle(queue, "idle")).toBe("second");
  expect(dequeueOnIdle(queue, "idle")).toBeUndefined(); // drained
});
