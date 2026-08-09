import { expect, test } from 'bun:test';
import {
  compactionImminentOf,
  contextPercentOf,
  readContext,
  readTurnSnapshots,
  readUsage,
  snapshotForSession,
} from '@/shared/state/deaconBus';

// The bug this file exists for: the ctx meter divided the last TURN'S SPEND
// (prompt + completion summed over every model call in the turn) by the context
// window. An agentic turn re-sends the prompt on each tool call, so a real
// session printed "ctx 388%" — 507,524 spend tokens against a 131,072 window —
// while the context was barely half full. Fill and spend are different
// quantities; the meter must read fill.
test('fill comes from turn.usage, and a spend-shaped payload yields no meter', () => {
  const fill = readContext({
    context_tokens: 44_000,
    max_context_tokens: 131_072,
    tool_schema_tokens: 5_200,
    compact_at_tokens: 65_536,
  });
  expect(contextPercentOf(fill)).toBe(34);

  // turn.complete's spend fields alone must never produce a snapshot — that is
  // exactly the payload that used to drive the meter.
  expect(readContext({ input_tokens: 507_524, output_tokens: 1_351, context_max: 131_072 })).toBeUndefined();
  expect(contextPercentOf(undefined)).toBeUndefined();
});

test('provider-reported spend stays distinct from estimated context fill', () => {
  const usage = readUsage({
    input_tokens: 30,
    output_tokens: 13,
    usage_complete: true,
    last_request_input_tokens: 20,
  });
  expect(usage).toEqual({
    inputTokens: 30,
    outputTokens: 13,
    complete: true,
    lastRequestInputTokens: 20,
  });
  expect(readUsage({ input_tokens: 0, output_tokens: 0, usage_complete: false })?.complete).toBe(false);
  expect(readUsage({ input_tokens: 1, output_tokens: 2 })?.complete).toBe(false);
  expect(readUsage({ input_tokens: -1, output_tokens: 2 })).toBeUndefined();
  expect(
    readUsage({ session_id: 'session-a', input_tokens: 1, output_tokens: 2 })?.sessionId,
  ).toBe('session-a');
  expect(
    readContext({ session_id: 'session-a', context_tokens: 3, max_context_tokens: 10 })?.sessionId,
  ).toBe('session-a');
});

test('status snapshots are atomic and visible only for the active session', () => {
  const paired = readTurnSnapshots({
    session_id: 'session-a',
    input_tokens: 3,
    output_tokens: 2,
    usage_complete: true,
    context_tokens: 10,
    max_context_tokens: 100,
  });
  expect(paired?.usage.sessionId).toBe('session-a');
  expect(paired?.context.sessionId).toBe('session-a');
  expect(snapshotForSession(paired?.usage, 'session-a')).toBe(paired?.usage);
  expect(snapshotForSession(paired?.usage, 'session-b')).toBeUndefined();
  expect(snapshotForSession(paired?.usage, undefined)).toBeUndefined();
  expect(
    readTurnSnapshots({ session_id: 'session-a', context_tokens: 10, max_context_tokens: 100 }),
  ).toBeUndefined();
});

test('a partial or zero-window payload is dropped rather than shown as a guess', () => {
  expect(readContext({ context_tokens: 44_000 })).toBeUndefined();
  expect(readContext({ context_tokens: 44_000, max_context_tokens: 0 })).toBeUndefined();
  expect(readContext({ context_tokens: -1, max_context_tokens: 10 })).toBeUndefined();
  expect(readContext({ context_tokens: Number.NaN, max_context_tokens: 10 })).toBeUndefined();
  // A missing tool-schema slice is not fatal — it defaults to 0.
  expect(readContext({ context_tokens: 10, max_context_tokens: 100 })?.toolSchemaTokens).toBe(0);
});

test('compaction warns at the threshold, not at a full window', () => {
  const at = (used: number) =>
    compactionImminentOf(
      readContext({ context_tokens: used, max_context_tokens: 131_072, compact_at_tokens: 65_536 }),
    );
  expect(at(65_535)).toBe(false);
  expect(at(65_536)).toBe(true); // the session splits on the next turn
  expect(at(70_000)).toBe(true);

  // Compaction disabled or breaker open: the backend sends null, so there is
  // nothing to warn about even at 99% full.
  const noThreshold = readContext({ context_tokens: 130_000, max_context_tokens: 131_072 });
  expect(noThreshold?.compactAtTokens).toBeUndefined();
  expect(compactionImminentOf(noThreshold)).toBe(false);
});
