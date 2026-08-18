'use client';
import { useLayoutEffect, type KeyboardEvent, type ReactNode, type RefObject } from 'react';

const DEFAULT_MAX_ROWS = 7;

export interface PromptInputBarProps {
  readonly value: string;
  readonly onChange: (value: string) => void;
  readonly onKeyDown?: (event: KeyboardEvent<HTMLTextAreaElement>) => void;
  readonly placeholder: string;
  readonly textareaRef: RefObject<HTMLTextAreaElement | null>;
  readonly ariaLabel?: string;
  readonly left?: ReactNode;
  readonly right?: ReactNode;
  readonly maxRows?: number;
  readonly disabled?: boolean;
}

function resizeTextarea(el: HTMLTextAreaElement, maxRows: number): void {
  const style = window.getComputedStyle(el);
  const parsedLineHeight = Number.parseFloat(style.lineHeight);
  const fontSize = Number.parseFloat(style.fontSize);
  const lineHeight = Number.isFinite(parsedLineHeight) ? parsedLineHeight : fontSize * 1.5;
  const paddingY = Number.parseFloat(style.paddingTop) + Number.parseFloat(style.paddingBottom);
  const borderY = Number.parseFloat(style.borderTopWidth) + Number.parseFloat(style.borderBottomWidth);
  const maxHeight = lineHeight * maxRows + paddingY + borderY;

  el.style.height = 'auto';
  el.style.height = `${Math.min(el.scrollHeight, maxHeight)}px`;
  el.style.overflowY = el.scrollHeight > maxHeight ? 'auto' : 'hidden';
}

export function PromptInputBar({
  value,
  onChange,
  onKeyDown,
  placeholder,
  textareaRef,
  ariaLabel,
  left,
  right,
  maxRows = DEFAULT_MAX_ROWS,
  disabled = false,
}: PromptInputBarProps) {
  useLayoutEffect(() => {
    const el = textareaRef.current;
    if (el === null) return;
    resizeTextarea(el, maxRows);
  }, [maxRows, textareaRef, value]);

  useLayoutEffect(() => {
    const el = textareaRef.current;
    if (el === null) return;
    const onResize = () => resizeTextarea(el, maxRows);
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, [maxRows, textareaRef]);

  return (
    <div
      // TWO ROWS: the message gets the full width, the controls sit under it.
      // On one row the textarea was `flex-1` against a fixed-width control
      // cluster (attach · butler · mic · model pill · send), so a long model
      // name — "nemotron-3-ultra-…" — left barely half the bar to type in, and
      // a pasted path wrapped after ~40 characters with a scrollbar beside it.
      // Shrinking the pill only moves the loss around; the text is the thing
      // that deserves the width.
      //
      // `min-w-0` still matters on both rows: it lets them absorb the squeeze
      // when the panel is dragged narrow. Deliberately NOT `overflow-hidden` —
      // that stopped the spill but clipped SEND off the end.
      className="flex min-w-0 flex-col gap-0.5 rounded-2xl bg-bg p-1.5"
      style={{ boxShadow: 'var(--shadow-prompt)' }}
    >
      <textarea
        ref={textareaRef}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={onKeyDown}
        placeholder={placeholder}
        rows={1}
        aria-label={ariaLabel ?? placeholder}
        disabled={disabled}
        className="w-full min-w-0 resize-none overflow-y-hidden bg-transparent px-2 pb-1 pt-1.5 text-sm text-text-primary outline-none placeholder:text-text-tertiary disabled:cursor-not-allowed disabled:opacity-60"
      />
      <div className="flex min-w-0 items-center gap-1.5">
        {left}
        {/* `ml-auto` + `min-w-0`: the cluster hugs the right edge and the model
            pill is the piece that gives way when there is no room. */}
        <div className="ml-auto flex min-w-0 items-center gap-1.5">{right}</div>
      </div>
    </div>
  );
}
