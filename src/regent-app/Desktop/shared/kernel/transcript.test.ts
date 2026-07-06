// Domain tests — zero I/O (Section 7): happy streaming path, non-streaming
// reply path, and the failure paths (turn error + submit failure).
import { describe, expect, test } from "bun:test";
import { type TranscriptState, emptyTranscript, reduceTranscript } from "./transcript";

const run = (events: Parameters<typeof reduceTranscript>[1][]): TranscriptState =>
  events.reduce(reduceTranscript, emptyTranscript);

describe("reduceTranscript", () => {
  test("streams deltas into one assistant item and seals on turn end", () => {
    const s = run([
      { type: "submitted", text: "hi" },
      { type: "delta", text: "Hel" },
      { type: "delta", text: "lo" },
      { type: "ended" },
    ]);
    expect(s.items).toEqual([
      { kind: "user", text: "hi" },
      { kind: "assistant", text: "Hello", streaming: false },
    ]);
    expect(s.busy).toBe(false);
  });

  test("non-streaming reply replaces the partial and busy tracks the turn", () => {
    const mid = run([{ type: "submitted", text: "q" }, { type: "delta", text: "par" }]);
    expect(mid.busy).toBe(true);
    const s = [{ type: "reply", text: "full answer" } as const, { type: "ended" } as const].reduce(
      reduceTranscript,
      mid,
    );
    expect(s.items.at(-1)).toEqual({ kind: "assistant", text: "full answer", streaming: false });
    expect(s.items.filter((i) => i.kind === "assistant")).toHaveLength(1);
  });

  test("turn error surfaces verbatim after the sealed partial", () => {
    const s = run([
      { type: "submitted", text: "q" },
      { type: "delta", text: "part" },
      { type: "ended", error: "402 insufficient credit" },
    ]);
    expect(s.items.at(-1)).toEqual({ kind: "error", message: "402 insufficient credit" });
    expect(s.items.at(-2)).toEqual({ kind: "assistant", text: "part", streaming: false });
    expect(s.busy).toBe(false);
  });

  test("seed fills an empty transcript but never clobbers live turns", () => {
    const seeded = run([
      { type: "seeded", items: [{ kind: "user", text: "old q" }] },
    ]);
    expect(seeded.items).toEqual([{ kind: "user", text: "old q" }]);
    const live = run([
      { type: "submitted", text: "new q" },
      { type: "seeded", items: [{ kind: "user", text: "old q" }] },
    ]);
    expect(live.items).toEqual([{ kind: "user", text: "new q" }]);
  });

  test("tool rows interleave per step and reply consolidates the turn's text", () => {
    const s = run([
      { type: "submitted", text: "do it" },
      { type: "delta", text: "Let me look. " },
      { type: "tool-start", name: "computer_use" },
      { type: "tool-end", name: "computer_use" },
      { type: "delta", text: "Found it." },
      { type: "reply", text: "Let me look. Found it." },
      { type: "ended" },
    ]);
    expect(s.items).toEqual([
      { kind: "user", text: "do it" },
      { kind: "tool", name: "computer_use", done: true, isError: undefined },
      { kind: "assistant", text: "Let me look. Found it.", streaming: false },
    ]);
  });

  test("approval flow: request renders, resolution marks it, turn continues", () => {
    const s = run([
      { type: "submitted", text: "close the tab" },
      { type: "approval", tool: "computer_use", action: "click (240,67)", reason: "browser action" },
      { type: "approval-resolved", approved: true },
      { type: "reply", text: "Done." },
      { type: "ended" },
    ]);
    expect(s.items[1]).toEqual({
      kind: "approval",
      tool: "computer_use",
      action: "click (240,67)",
      reason: "browser action",
      resolved: "approved",
    });
    expect(s.items.at(-1)).toEqual({ kind: "assistant", text: "Done.", streaming: false });
  });

  test("submit failure lands as an error item and clears busy", () => {
    const s = run([
      { type: "submitted", text: "q" },
      { type: "failed", message: "deacon is not running" },
    ]);
    expect(s.items.at(-1)).toEqual({ kind: "error", message: "deacon is not running" });
    expect(s.busy).toBe(false);
  });
});
