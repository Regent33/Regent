'use client';
// The chat composer — a floating rounded surface (borderless + shadow, like
// Hermes'): attach · auto-growing textarea · mic · circular send/stop.
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
    <div className="relative mx-auto mb-5 w-full max-w-[680px] px-6">
      <div
        className="flex items-end gap-1.5 rounded-2xl bg-bg py-1.5 pl-2 pr-1.5"
        style={{ boxShadow: 'var(--shadow-elev)' }}
      >
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

        {busy ? (
          <Button
            variant="default"
            size="icon"
            aria-label={s.stop}
            className="size-9 rounded-full"
            onClick={onStop}
          >
            <StopIcon />
          </Button>
        ) : (
          <Button
            variant="default"
            size="icon"
            aria-label={s.send}
            className="size-9 rounded-full"
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
