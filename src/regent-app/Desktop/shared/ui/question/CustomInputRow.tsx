// The "Something else…" escape hatch: a normal option row that expands into an
// inline textarea. Enter sends, Shift+Enter adds a newline — the same contract
// as the composer, so the muscle memory carries over.
//
// A `text` question has no options at all; the card then renders this with no
// row above it (`index` undefined) and the box already open.
import { useEffect, useRef, useState } from 'react';
import { Button } from '@/shared/ui/Button';
import { t } from '@/shared/i18n/t';
import { OptionRow } from '@/shared/ui/question/OptionRow';

export interface CustomInputRowProps {
  id: string;
  /** 0-based row index, or undefined for a `text` question with no rows. */
  index?: number;
  multi: boolean;
  cursor: boolean;
  open: boolean;
  onOpen: () => void;
  onSubmit: (text: string) => void;
  /** Collapse the box and hand the keyboard back to the card. */
  onCancel: () => void;
}

export function CustomInputRow({
  id,
  index,
  multi,
  cursor,
  open,
  onOpen,
  onSubmit,
  onCancel,
}: CustomInputRowProps) {
  const s = t().chat.question;
  const [text, setText] = useState('');
  const boxRef = useRef<HTMLTextAreaElement | null>(null);

  useEffect(() => {
    if (open) boxRef.current?.focus({ preventScroll: true });
  }, [open]);

  const send = () => {
    const trimmed = text.trim();
    if (trimmed !== '') onSubmit(trimmed);
  };

  return (
    <div>
      {index !== undefined && (
        <OptionRow
          id={id}
          index={index}
          label={s.custom}
          multi={multi}
          checked={open}
          cursor={cursor}
          onSelect={onOpen}
        />
      )}
      {open && (
        <div className="mt-1.5 rounded-md border border-stroke-tertiary bg-surface p-2">
          <textarea
            ref={boxRef}
            rows={2}
            value={text}
            placeholder={s.customPlaceholder}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Escape') {
                e.preventDefault();
                onCancel();
                return;
              }
              if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                send();
              }
            }}
            className="w-full resize-y bg-transparent text-sm leading-relaxed text-text-primary outline-none placeholder:text-text-tertiary"
          />
          <div className="mt-1 flex items-center justify-between gap-2">
            <span className="text-[11px] text-text-tertiary">{s.customHint}</span>
            <Button size="sm" disabled={text.trim() === ''} onClick={send}>
              {s.customSubmit}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
