// The headless machine contract. It becomes a public automation API the moment
// `--events` ships, so it is pinned here rather than discovered by whoever
// scripts against it first.
import { expect, test } from "bun:test";
import {
  ASK_EXIT,
  ASK_INITIAL,
  type AskState,
  askExitCode,
  reduceAsk,
  resolveApproval,
  terminalEvent,
} from "./askRun.ts";

const feed = (events: Array<[string, Record<string, unknown>]>): AskState =>
  events.reduce((s, [m, p]) => reduceAsk(s, m, p), ASK_INITIAL);

test("deltas accumulate and the final reply replaces them", () => {
  const s = feed([
    ["message.delta", { text: "Hel" }],
    ["message.delta", { text: "lo" }],
    ["message.complete", { reply: "Hello, world." }],
    ["turn.complete", {}],
  ]);
  // Appending the authoritative reply to the streamed preview would print the
  // answer twice — the interactive surface has the same rule.
  expect(s.answer).toBe("Hello, world.");
  expect(s.done).toBe(true);
});

test("a completion with no reply keeps whatever streamed", () => {
  const s = feed([
    ["message.delta", { text: "partial" }],
    ["message.complete", {}],
  ]);
  expect(s.answer).toBe("partial");
});

// The plan's exit-code table originally conflated these two. A run that is
// refused a write and still answers has done its job.
test("a denied approval is not a failed run", () => {
  let s = feed([["approval.request", { tool: "terminal", action: "rm -rf x" }]]);
  expect(s.pendingApproval).toEqual({ tool: "terminal", action: "rm -rf x" });
  s = resolveApproval(s, false);
  s = reduceAsk(s, "message.complete", { reply: "I could not delete it, but here is why." });
  s = reduceAsk(s, "turn.complete", {});
  expect(s.deniedApprovals).toBe(1);
  expect(askExitCode(s)).toBe(ASK_EXIT.ok);
  // …and the real outcome is still visible to automation.
  const ev = terminalEvent(s, { runId: "r", sessionId: "s" });
  expect(ev.denied_approvals).toBe(1);
  expect(ev.status).toBe("completed");
});

test("approving clears the pending approval without counting a denial", () => {
  const s = resolveApproval(feed([["approval.request", { tool: "t", action: "a" }]]), true);
  expect(s.pendingApproval).toBeNull();
  expect(s.deniedApprovals).toBe(0);
});

test("exit codes follow the taxonomy", () => {
  const withStatus = (status: AskState["status"]): AskState => ({ ...ASK_INITIAL, status });
  expect(askExitCode(withStatus("completed"))).toBe(0);
  expect(askExitCode(withStatus("failed"))).toBe(1);
  expect(askExitCode(withStatus("timed_out"))).toBe(3);
  expect(askExitCode(withStatus("denied_by_policy"))).toBe(3);
  expect(askExitCode(withStatus("interrupted"))).toBe(130);
  // Usage and unavailable are decided before a run exists, so they are not
  // reachable from a run state — but they must not collide with these.
  expect(new Set(Object.values(ASK_EXIT)).size).toBe(Object.values(ASK_EXIT).length);
});

test("an interrupted turn is terminal and exits 130", () => {
  const s = feed([
    ["message.delta", { text: "half an ans" }],
    ["turn.interrupted", {}],
  ]);
  expect(s.done).toBe(true);
  expect(askExitCode(s)).toBe(130);
  const ev = terminalEvent(s, { runId: "r", sessionId: "s" });
  // The partial answer is already on stdout, so a script has to be able to see
  // that it is partial. That is exactly what this field is for.
  expect(ev.answer_complete).toBe(false);
});

test("tool calls are counted for the terminal event", () => {
  const s = feed([
    ["tool.start", { tool: "read_file" }],
    ["tool.start", { tool: "search_files" }],
    ["turn.complete", {}],
  ]);
  expect(terminalEvent(s, { runId: "r", sessionId: "s" }).tool_calls).toBe(2);
});

test("the terminal event carries a schema version and both ids", () => {
  const ev = terminalEvent(ASK_INITIAL, { runId: "run-1", sessionId: "sess-1" });
  expect(ev).toMatchObject({
    type: "run.completed",
    schema_version: 1,
    run_id: "run-1",
    session_id: "sess-1",
  });
});

test("unknown events are ignored rather than breaking the run", () => {
  // The deacon emits events this consumer does not model (turn.usage, notes).
  // A headless run must not fall over on one it has never seen.
  const s = feed([
    ["turn.usage", { context_tokens: 10 }],
    ["something.new", { whatever: true }],
    ["message.complete", { reply: "fine" }],
    ["turn.complete", {}],
  ]);
  expect(s.answer).toBe("fine");
  expect(askExitCode(s)).toBe(0);
});

// A run that never reaches a terminal event has NOT succeeded. Starting the
// status at "completed" meant a dead deacon, a closed pipe or a dropped
// notification exited 0 with a partial answer and no indication anything was
// wrong — the worst possible outcome for a script.
test("a run with no terminal event fails instead of inheriting success", () => {
  const s = feed([
    ["turn.started", {}],
    ["message.delta", { text: "half an answer" }],
  ]);
  expect(s.status).toBe("running");
  expect(s.done).toBe(false);
  expect(askExitCode(s)).toBe(ASK_EXIT.failure);
  expect(terminalEvent(s, { runId: "r", sessionId: "s" }).answer_complete).toBe(false);
});

test("turn.complete promotes a running turn but never overwrites a real outcome", () => {
  expect(feed([["turn.complete", {}]]).status).toBe("completed");
  // An interrupt followed by a completion must stay interrupted.
  expect(
    feed([
      ["turn.interrupted", {}],
      ["turn.complete", {}],
    ]).status,
  ).toBe("interrupted");
});
