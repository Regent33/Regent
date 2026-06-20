import { expect, test } from "bun:test";
import { type ChatState, initialChatState, reduceChat } from "./transcript.ts";

// Fold a sequence of actions, like the viewmodel does for live events.
function run(actions: Parameters<typeof reduceChat>[1][]): ChatState {
  return actions.reduce(reduceChat, initialChatState);
}

const event = (method: string, params: Record<string, unknown> = {}) =>
  ({ type: "daemonEvent", method, params }) as const;

test("a streamed turn commits the buffer and returns to idle", () => {
  const s = run([
    { type: "userMessage", text: "hi" },
    event("turn.started"),
    event("message.delta", { text: "Hel" }),
    event("message.delta", { text: "lo" }),
    event("turn.complete"),
  ]);
  expect(s.phase).toBe("idle");
  expect(s.streaming).toBe("");
  expect(s.entries.map((e) => e.kind)).toEqual(["user", "assistant"]);
  const assistant = s.entries[1];
  expect(assistant?.kind === "assistant" && assistant.text).toBe("Hello");
});

test("tool.start commits in-flight stream then logs an activity line", () => {
  const s = run([
    event("message.delta", { text: "thinking" }),
    event("tool.start", { tool: "web_search" }),
  ]);
  // streamed text committed as assistant, then the tool entry
  expect(s.entries.map((e) => e.kind)).toEqual(["assistant", "tool"]);
  const tool = s.entries[1];
  expect(tool?.kind === "tool" && tool.tool).toBe("web_search");
  expect(s.streamingActive).toBe(false);
});

test("tool.complete only records an entry on error", () => {
  const ok = run([event("tool.complete", { tool: "patch", is_error: false })]);
  expect(ok.entries).toHaveLength(0);
  const failed = run([event("tool.complete", { tool: "patch", is_error: true })]);
  expect(failed.entries.map((e) => e.kind)).toEqual(["toolError"]);
});

test("approval.request enters approving and resolving returns to busy", () => {
  const asked = run([event("approval.request", { tool: "shell", action: "rm -rf /tmp/x" })]);
  expect(asked.phase).toBe("approving");
  expect(asked.approval).toEqual({ tool: "shell", action: "rm -rf /tmp/x" });
  expect(asked.entries.map((e) => e.kind)).toEqual(["approvalAsk"]);

  const resolved = reduceChat(asked, { type: "approvalResolved", approved: true });
  expect(resolved.phase).toBe("busy");
  expect(resolved.approval).toBeNull();
  expect(resolved.entries.at(-1)?.kind).toBe("approvalResolved");
});

test("message.complete appends a non-streamed reply", () => {
  const s = run([event("message.complete", { reply: "done" })]);
  expect(s.entries.map((e) => e.kind)).toEqual(["assistant"]);
  const a = s.entries[0];
  expect(a?.kind === "assistant" && a.text).toBe("done");
});

test("message.complete after streaming commits the stream, not the reply field", () => {
  const s = run([
    event("message.delta", { text: "streamed" }),
    event("message.complete", { reply: "ignored" }),
  ]);
  expect(s.entries).toHaveLength(1);
  const a = s.entries[0];
  expect(a?.kind === "assistant" && a.text).toBe("streamed");
});

test("turn.interrupted commits the stream, notes it, and clears busy", () => {
  const s = run([event("message.delta", { text: "partial" }), event("turn.interrupted")]);
  expect(s.phase).toBe("idle");
  expect(s.entries.map((e) => e.kind)).toEqual(["assistant", "note"]);
});

test("entry ids are unique and monotonic (stable keys for <Static>)", () => {
  const s = run([
    { type: "userMessage", text: "a" },
    event("tool.start", { tool: "t" }),
    event("message.complete", { reply: "b" }),
  ]);
  const ids = s.entries.map((e) => e.id);
  expect(new Set(ids).size).toBe(ids.length);
  expect(ids).toEqual([...ids].sort((x, y) => x - y));
});
