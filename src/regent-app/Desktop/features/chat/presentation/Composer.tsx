'use client';
// The chat composer — attach/mic placeholders, an auto-growing textarea, a
// pulsing voice-orb placeholder (M3 wires the real animated core), and the
// send/stop action. Flat surface with a hairline top edge (flat-not-boxed).
import { useRef, useState, type KeyboardEvent } from 'react';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { MicIcon, PaperclipIcon, SendIcon, StopIcon } from '@/shared/ui/icons';

const MIN_ROWS = 1;
const MAX_ROWS = 6;

export interface ComposerProps {
  busy: boolean;
  onSubmit: (text: string) => void;
  onStop: () => void;
}

export function Composer({ busy, onSubmit, onStop }: ComposerProps) {
  const s = t().chat.composer;
  const [value, setValue] = useState('');
  const [rows, setRows] = useState(MIN_ROWS);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const growToFit = (el: HTMLTextAreaElement) => {
    const lineCount = el.value.split('\n').length;
    setRows(Math.min(MAX_ROWS, Math.max(MIN_ROWS, lineCount)));
  };

  const submit = () => {
    const text = value.trim();
    if (text === '' || busy) return;
    onSubmit(text);
    setValue('');
    setRows(MIN_ROWS);
    textareaRef.current?.focus();
  };

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      submit();
    }
  };

  return (
    <div className="mx-auto w-full max-w-[760px] border-t border-stroke-tertiary px-4 py-3">
      <div className="flex items-end gap-2">
        <Button variant="ghost" size="icon" aria-label={s.attach} disabled>
          <PaperclipIcon />
        </Button>

        <textarea
          ref={textareaRef}
          value={value}
          onChange={(e) => {
            setValue(e.target.value);
            growToFit(e.target);
          }}
          onKeyDown={onKeyDown}
          rows={rows}
          placeholder={s.placeholder}
          className="min-w-0 flex-1 resize-none bg-transparent py-2 text-sm text-text-primary outline-none placeholder:text-text-tertiary"
        />

        <Button variant="ghost" size="icon" aria-label={s.mic} disabled>
          <MicIcon />
        </Button>

        <span aria-hidden className="mb-2.5 size-2.5 shrink-0 rounded-full bg-accent motion-safe:animate-pulse" />

        {busy ? (
          <Button variant="default" size="icon" aria-label={s.stop} className="min-h-11 min-w-11" onClick={onStop}>
            <StopIcon />
          </Button>
        ) : (
          <Button
            variant="default"
            size="icon"
            aria-label={s.send}
            className="min-h-11 min-w-11"
            disabled={value.trim() === ''}
            onClick={submit}
          >
            <SendIcon />
          </Button>
        )}
      </div>
    </div>
  );
}
