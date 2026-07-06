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

  test("submit failure lands as an error item and clears busy", () => {
    const s = run([
      { type: "submitted", text: "q" },
      { type: "failed", message: "deacon is not running" },
    ]);
    expect(s.items.at(-1)).toEqual({ kind: "error", message: "deacon is not running" });
    expect(s.busy).toBe(false);
  });
});
