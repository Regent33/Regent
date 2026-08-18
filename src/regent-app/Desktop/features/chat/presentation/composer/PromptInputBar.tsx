'use client';
import { useLayoutEffect, useRef, type KeyboardEvent, type ReactNode, type RefObject } from 'react';

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
  /** Fires when the message stops (or starts) fitting on one line, so the
   *  caller can step its wide controls out of the way. */
  readonly onExpandedChange?: (expanded: boolean) => void;
}

/** Grows the field to fit its text, up to `maxRows`, and reports whether that
 *  text still fits on ONE line.
 *
 *  `probeWidth` is the width the field has while the collapsible controls are
 *  showing; 0 means "measure at whatever width it has right now". Both states
 *  must be judged at that one width, or the measurement feeds back on itself:
 *  hiding the controls widens the field, the text now fits on one line, the
 *  controls come back, it wraps again. That is not an edge case — the field is
 *  ~280px with the controls and ~480px without, so every message between those
 *  two widths would sit in the loop and flicker. */
function resizeTextarea(el: HTMLTextAreaElement, maxRows: number, probeWidth: number): boolean {
  const style = window.getComputedStyle(el);
  const parsedLineHeight = Number.parseFloat(style.lineHeight);
  const fontSize = Number.parseFloat(style.fontSize);
  const lineHeight = Number.isFinite(parsedLineHeight) ? parsedLineHeight : fontSize * 1.5;
  const paddingY = Number.parseFloat(style.paddingTop) + Number.parseFloat(style.paddingBottom);
  const borderY = Number.parseFloat(style.borderTopWidth) + Number.parseFloat(style.borderBottomWidth);
  const oneRow = lineHeight + paddingY + borderY;
  const maxHeight = lineHeight * maxRows + paddingY + borderY;

  el.style.height = 'auto';
  let wrapped = el.scrollHeight > oneRow + 1;
  if (probeWidth > 0 && Math.abs(el.clientWidth - probeWidth) > 1) {
    el.style.width = `${probeWidth}px`;
    wrapped = el.scrollHeight > oneRow + 1;
    el.style.width = '';
  }
  el.style.height = `${Math.min(el.scrollHeight, maxHeight)}px`;
  el.style.overflowY = el.scrollHeight > maxHeight ? 'auto' : 'hidden';
  return wrapped;
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
  onExpandedChange,
}: PromptInputBarProps) {
  const expandedRef = useRef(false);
  const narrowWidthRef = useRef(0);

  const measure = (el: HTMLTextAreaElement) => {
    const wrapped = resizeTextarea(el, maxRows, expandedRef.current ? narrowWidthRef.current : 0);
    if (!expandedRef.current) {
      // Only ever keep the SMALLER sample. For the 200ms after a collapse the
      // controls are still sliding back in, so clientWidth reads too wide, and
      // one too-wide sample would go on to judge a wrapped message as fitting.
      // Erring narrow costs an early collapse; erring wide costs the flicker.
      narrowWidthRef.current =
        narrowWidthRef.current === 0
          ? el.clientWidth
          : Math.min(narrowWidthRef.current, el.clientWidth);
    }
    if (wrapped !== expandedRef.current) {
      expandedRef.current = wrapped;
      onExpandedChange?.(wrapped);
    }
  };

  useLayoutEffect(() => {
    const el = textareaRef.current;
    if (el === null) return;
    measure(el);
  });

  useLayoutEffect(() => {
    const el = textareaRef.current;
    if (el === null) return;
    const onResize = () => {
      narrowWidthRef.current = 0; // the panel changed size — learn it again
      measure(el);
    };
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, [textareaRef]);

  return (
    <div
      // ONE row: the message sits between the paperclip and Send, not stacked
      // above them. A one-row bar used to leave barely half the width to type
      // in, because a long model name — "nemotron-3-ultra-…" — held its space
      // no matter how much was being written. The answer is for those controls
      // to leave once the message needs the room (see `onExpandedChange`), not
      // for the text to move to a line of its own.
      //
      // `items-end` keeps the buttons beside the LAST line as the field grows.
      // `min-w-0` lets the row absorb the squeeze when the panel is dragged
      // narrow. Deliberately NOT `overflow-hidden` — that stopped the spill
      // but clipped SEND off the end.
      className="flex min-w-0 items-end gap-1.5 rounded-2xl bg-bg px-2.5 py-2"
      style={{ boxShadow: 'var(--shadow-prompt)' }}
    >
      {left}
      <textarea
        ref={textareaRef}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={onKeyDown}
        placeholder={placeholder}
        rows={1}
        aria-label={ariaLabel ?? placeholder}
        disabled={disabled}
        // py-2 makes one row exactly as tall as the 36px buttons beside it, so
        // the text is centred against them on one line and sits level with the
        // last line on many.
        className="min-w-0 flex-1 resize-none overflow-y-hidden bg-transparent px-1.5 py-2 text-sm text-text-primary outline-none placeholder:text-text-tertiary disabled:cursor-not-allowed disabled:opacity-60"
      />
      {right}
    </div>
  );
}
