import { describe, expect, test } from 'bun:test';
import {
  MAX_LINES,
  NO_LINES,
  appendLines,
  logTone,
  toolLine,
} from '@/features/workspace/domain/outputLines';

const texts = (lines: readonly { text: string }[]) => lines.map((l) => l.text);

describe('appendLines', () => {
  test('appends in order with unique ids', () => {
    const one = appendLines(NO_LINES, ['a', 'b']);
    expect(texts(one)).toEqual(['a', 'b']);
    const two = appendLines(one, ['c']);
    expect(texts(two)).toEqual(['a', 'b', 'c']);
    expect(new Set(two.map((l) => l.id)).size).toBe(3);
  });

  // Identity, not equality: a no-op that returns a fresh array re-renders the
  // panel on every unrelated event.
  test('an empty append returns the same array', () => {
    const one = appendLines(NO_LINES, ['a']);
    expect(appendLines(one, [])).toBe(one);
  });

  test('the buffer is bounded and drops from the head', () => {
    const many = Array.from({ length: MAX_LINES + 50 }, (_, i) => `line ${i}`);
    const filled = appendLines(NO_LINES, many);
    expect(filled.length).toBe(MAX_LINES);
    expect(filled[0].text).toBe('line 50');
    expect(filled[filled.length - 1].text).toBe(`line ${MAX_LINES + 49}`);
  });

  // The head drop is exactly where a naive index-based key collides.
  test('ids stay unique across a drop', () => {
    let lines = appendLines(NO_LINES, Array.from({ length: MAX_LINES }, (_, i) => `${i}`));
    lines = appendLines(lines, ['fresh']);
    expect(new Set(lines.map((l) => l.id)).size).toBe(lines.length);
  });
});

describe('logTone', () => {
  test('reads the level out of a tracing line', () => {
    expect(logTone('2026-07-30T23:00:00+08:00  ERROR regent: boom')).toBe('error');
    expect(logTone('2026-07-30T23:00:00+08:00  WARN regent: hmm')).toBe('error');
    expect(logTone('2026-07-30T23:00:00+08:00  INFO regent: fine')).toBe('normal');
    expect(logTone('2026-07-30T23:00:00+08:00  DEBUG regent: noisy')).toBe('muted');
  });

  // A module path containing the word must not paint the line red.
  test('a logger named "error" is not an error', () => {
    expect(logTone('2026-07-30T23:00:00+08:00  INFO regent_tools::infra::error: ok')).toBe('normal');
  });
});

describe('toolLine', () => {
  test('start and complete are separate lines', () => {
    expect(toolLine('tool.start', { tool: 'read_file', args_summary: '{"path":"a"}' })).toEqual({
      text: '→ read_file {"path":"a"}',
      tone: 'muted',
    });
    expect(toolLine('tool.complete', { tool: 'read_file', ok: true, result_summary: '12 lines' }))
      .toEqual({ text: '✓ read_file 12 lines', tone: 'normal' });
  });

  test('a failure is marked and coloured', () => {
    expect(toolLine('tool.complete', { tool: 'web_fetch', ok: false })?.tone).toBe('error');
    // `ok` is newer than `is_error`; a deacon one version behind sends only the
    // latter, and the panel must not report its failures as successes.
    expect(toolLine('tool.complete', { tool: 'web_fetch', is_error: true })?.tone).toBe('error');
  });

  test('anything without a tool name is not a line', () => {
    expect(toolLine('tool.start', {})).toBeUndefined();
    expect(toolLine('turn.complete', { tool: 'x' })).toBeUndefined();
  });
});
