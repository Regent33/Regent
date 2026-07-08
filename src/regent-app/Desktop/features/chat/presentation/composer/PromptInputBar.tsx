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
      className="flex items-end gap-1.5 rounded-2xl bg-bg py-1.5 pl-2 pr-1.5"
      style={{ boxShadow: 'var(--shadow-elev)' }}
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
        className="min-w-0 flex-1 resize-none overflow-y-hidden bg-transparent py-2 text-sm text-text-primary outline-none placeholder:text-text-tertiary disabled:cursor-not-allowed disabled:opacity-60"
      />
      {right}
    </div>
  );
}
