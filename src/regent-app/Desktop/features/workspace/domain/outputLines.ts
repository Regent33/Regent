// The Output panel's pure state: what a line is, and how a bounded log grows.
//
// Shared by both channels and by the Debug Console, because "append forever,
// but never without a ceiling" is the same problem in all three. A panel that
// keeps every line of a long session is a memory leak with a scrollbar.

/** How a line reads. `error` is coloured; the rest differ only in emphasis. */
export type LineTone = 'normal' | 'muted' | 'error';

export interface OutputLine {
  /** Monotonic — React keys must not collide when the buffer drops its head. */
  readonly id: number;
  readonly text: string;
  readonly tone: LineTone;
}

/** Lines kept per channel. ~10x a screenful at any sane panel height, and the
 * oldest are the ones nobody scrolls back to. */
export const MAX_LINES = 1_000;

export const NO_LINES: readonly OutputLine[] = [];

/** Append, dropping the oldest once full. Returns the SAME array when there is
 * nothing to add, so a no-op event cannot trigger a re-render. */
export function appendLines(
  current: readonly OutputLine[],
  texts: readonly string[],
  tone: LineTone = 'normal',
): readonly OutputLine[] {
  if (texts.length === 0) return current;
  // Counted off the last id, not the length — the buffer drops its head, so
  // length-based ids would repeat and collide as React keys.
  const base = (current[current.length - 1]?.id ?? 0) + 1;
  const added = texts.map((text, index) => ({ id: base + index, text, tone }));
  const grown = [...current, ...added];
  return grown.length <= MAX_LINES ? grown : grown.slice(grown.length - MAX_LINES);
}

/** Tone for one deacon log line, read from the level tracing prints.
 *
 * Substring, not a prefix match: every line starts with an RFC-3339 timestamp,
 * so ` ERROR ` sits mid-line. The spaces matter — a logger name containing
 * "error" (`regent_tools::infra::error`) must not paint the whole line red. */
export function logTone(line: string): LineTone {
  if (line.includes(' ERROR ')) return 'error';
  if (line.includes(' WARN ')) return 'error';
  if (line.includes(' DEBUG ') || line.includes(' TRACE ')) return 'muted';
  return 'normal';
}

/** One agent tool event as a line. `tool.start` opens it, `tool.complete`
 * closes it with the outcome — two lines, because they can be minutes apart
 * and a single mutated line would hide how long the tool actually ran. */
export function toolLine(
  method: string,
  params: Record<string, unknown>,
): { text: string; tone: LineTone } | undefined {
  const tool = typeof params.tool === 'string' ? params.tool : undefined;
  if (tool === undefined) return undefined;
  if (method === 'tool.start') {
    const args = typeof params.args_summary === 'string' ? params.args_summary : '';
    return { text: `→ ${tool}${args === '' ? '' : ` ${args}`}`, tone: 'muted' };
  }
  if (method === 'tool.complete') {
    // `ok` is the newer field; `is_error` is what older deacons send. Reading
    // both keeps the panel honest against a backend one version behind.
    const failed = params.ok === false || params.is_error === true;
    const summary = typeof params.result_summary === 'string' ? params.result_summary : '';
    return {
      text: `${failed ? '✗' : '✓'} ${tool}${summary === '' ? '' : ` ${summary}`}`,
      tone: failed ? 'error' : 'normal',
    };
  }
  return undefined;
}
