'use client';
// The scrolling monospace log both the Output tab and the Debug Console render
// into. Follows the tail unless you have scrolled up — reading line 400 while
// new lines arrive must not yank you back to the bottom.
import { useEffect, useRef } from 'react';
import type { OutputLine } from '@/features/workspace/domain/outputLines';

const TONE: Record<OutputLine['tone'], string> = {
  normal: 'text-text-secondary',
  muted: 'text-text-tertiary',
  error: 'text-danger',
};

/** Within this many px of the bottom still counts as "at the bottom" — an
 * exact comparison fails on fractional scroll heights at browser zoom. */
const STICK_SLACK = 24;

export function LogView({ lines, empty }: { lines: readonly OutputLine[]; empty: string }) {
  const box = useRef<HTMLDivElement>(null);
  const stick = useRef(true);

  useEffect(() => {
    const element = box.current;
    // `lines.length` is the thing that moved, and reading it here is what makes
    // `[lines]` an honest dependency — an effect that declares a dep it never
    // touches reads as a stray, and the lint autofix for that is to DELETE the
    // dep, which would strand the log on its first screen forever.
    if (element === null || lines.length === 0 || !stick.current) return;
    element.scrollTop = element.scrollHeight;
  }, [lines]);

  if (lines.length === 0) {
    return <p className="p-3 text-[12px] text-text-tertiary">{empty}</p>;
  }
  return (
    <div
      ref={box}
      // leading-[1.5], not leading-1.5: on Tailwind v4 the bare-number form is
      // the SPACING multiplier (0.375rem = 6px), which collapsed 11px log lines
      // on top of each other. The arbitrary form is the unitless ratio.
      className="min-h-0 flex-1 overflow-auto px-3 py-2 font-mono text-[11px] leading-[1.5]"
      onScroll={(e) => {
        const { scrollTop, scrollHeight, clientHeight } = e.currentTarget;
        stick.current = scrollHeight - scrollTop - clientHeight < STICK_SLACK;
      }}
    >
      {lines.map((line) => (
        // `break-all`: log lines carry paths and base64 with no spaces to wrap
        // at, and one long line must not give the whole panel a scrollbar.
        <div key={line.id} className={`whitespace-pre-wrap break-all ${TONE[line.tone]}`}>
          {line.text}
        </div>
      ))}
    </div>
  );
}
