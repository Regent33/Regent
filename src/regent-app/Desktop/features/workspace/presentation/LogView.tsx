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
    if (element === null || !stick.current) return;
    element.scrollTop = element.scrollHeight;
  }, [lines]);

  if (lines.length === 0) {
    return <p className="p-3 text-[12px] text-text-tertiary">{empty}</p>;
  }
  return (
    <div
      ref={box}
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
