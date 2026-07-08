'use client';
// The chat composer — a floating rounded surface (borderless + shadow, like
// Hermes'): attach · auto-growing textarea · mic · model pill · circular
// send/stop (+ elapsed time while a turn runs). `/` at the start of an
// otherwise-empty line pops a command-completion menu; ↑/↓ on an
// empty/unedited composer cycles this session's prompt history.
import { useRef, useState, type KeyboardEvent } from 'react';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { MicIcon, PaperclipIcon, SendIcon, StopIcon } from '@/shared/ui/icons';
import { useTurnActivity } from '@/shared/state/deaconBus';
import { useInputHistory } from '@/features/chat/viewmodels/useInputHistory';
import { useSlashMenu } from '@/features/chat/viewmodels/useSlashMenu';
import { useElapsedSeconds } from '@/features/chat/viewmodels/useElapsedSeconds';
import { ModelPill } from '@/features/chat/presentation/composer/ModelPill';
import { SlashMenu } from '@/features/chat/presentation/composer/SlashMenu';

const MIN_ROWS = 1;
const MAX_ROWS = 6;

export interface ComposerProps {
  busy: boolean;
  sessionId: string | undefined;
  onSubmit: (text: string, attachments?: readonly File[]) => void;
  onStop: () => void;
}

const MAX_ATTACH_BYTES = 20 * 1024 * 1024; // mirrors the deacon's decoded cap

function rowsFor(value: string): number {
  const lineCount = value.split('\n').length;
  return Math.min(MAX_ROWS, Math.max(MIN_ROWS, lineCount));
}

function formatElapsed(totalSeconds: number): string {
  if (totalSeconds < 60) return `${totalSeconds}s`;
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${String(seconds).padStart(2, '0')}`;
}

export function Composer({ busy, sessionId, onSubmit, onStop }: ComposerProps) {
  const s = t().chat.composer;
  const [value, setValue] = useState('');
  const [rows, setRows] = useState(MIN_ROWS);
  const [files, setFiles] = useState<readonly File[]>([]);
  const [attachError, setAttachError] = useState<string>();
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const history = useInputHistory();

  const setText = (next: string) => {
    setValue(next);
    setRows(rowsFor(next));
  };

  const slash = useSlashMenu(value, setText, () => textareaRef.current?.focus());

  const elapsed = useElapsedSeconds(useTurnActivity(sessionId) === 'running');

  const submit = () => {
    const text = value.trim();
    // A message needs text OR at least one attachment; never send while busy.
    if ((text === '' && files.length === 0) || busy) return;
    onSubmit(text, files.length > 0 ? files : undefined);
    if (text !== '') history.record(text);
    setText('');
    setFiles([]);
    slash.reset();
    textareaRef.current?.focus();
  };

  const addFiles = (picked: FileList | null) => {
    if (picked === null) return;
    setAttachError(undefined);
    const accepted: File[] = [];
    for (const file of Array.from(picked)) {
      if (file.size > MAX_ATTACH_BYTES) setAttachError(s.attachTooBig);
      else accepted.push(file);
    }
    if (accepted.length > 0) setFiles((prev) => [...prev, ...accepted]);
    if (fileInputRef.current) fileInputRef.current.value = ''; // allow re-pick
  };

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (slash.onKeyDown(e)) return;
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      submit();
      return;
    }
    if (e.key === 'ArrowUp') {
      const next = history.up(value);
      if (next !== undefined) {
        e.preventDefault();
        setText(next);
      }
      return;
    }
    if (e.key === 'ArrowDown') {
      const next = history.down(value);
      if (next !== undefined) {
        e.preventDefault();
        setText(next);
      }
    }
  };

  return (
    <div className="relative mx-auto mb-5 w-full max-w-[680px] px-6">
      {slash.open && <SlashMenu items={slash.items} selected={slash.selected} onPick={slash.accept} />}

      {(files.length > 0 || attachError !== undefined) && (
        <div className="mb-1.5 flex flex-wrap items-center gap-1.5 px-1">
          {files.map((file, i) => (
            <span
              key={`${file.name}-${i}`}
              className="inline-flex items-center gap-1 rounded-full bg-hover px-2 py-0.5 text-xs text-text-secondary"
            >
              {file.name}
              <button
                type="button"
                aria-label={s.attachRemove}
                className="text-text-tertiary hover:text-text-primary"
                onClick={() => setFiles((prev) => prev.filter((_, j) => j !== i))}
              >
                ×
              </button>
            </span>
          ))}
          {attachError !== undefined && <span className="text-xs text-danger">{attachError}</span>}
        </div>
      )}

      <div
        className="flex items-end gap-1.5 rounded-2xl bg-bg py-1.5 pl-2 pr-1.5"
        style={{ boxShadow: 'var(--shadow-elev)' }}
      >
        <input
          ref={fileInputRef}
          type="file"
          multiple
          className="hidden"
          onChange={(e) => addFiles(e.target.files)}
        />
        <Button
          variant="ghost"
          size="icon"
          aria-label={s.attach}
          disabled={busy}
          onClick={() => fileInputRef.current?.click()}
        >
          <PaperclipIcon />
        </Button>

        <textarea
          ref={textareaRef}
          value={value}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={onKeyDown}
          rows={rows}
          placeholder={s.placeholder}
          className="min-w-0 flex-1 resize-none bg-transparent py-2 text-sm text-text-primary outline-none placeholder:text-text-tertiary"
        />

        <Button variant="ghost" size="icon" aria-label={s.mic} disabled>
          <MicIcon />
        </Button>

        <ModelPill disabled={busy} />

        {busy ? (
          <div className="flex items-center gap-1.5">
            {elapsed !== undefined && (
              <span className="tabular-nums text-xs text-text-tertiary">{formatElapsed(elapsed)}</span>
            )}
            <Button
              variant="default"
              size="icon"
              aria-label={s.stop}
              className="size-9 rounded-full"
              onClick={onStop}
            >
              <StopIcon />
            </Button>
          </div>
        ) : (
          <Button
            variant="default"
            size="icon"
            aria-label={s.send}
            className="size-9 rounded-full"
            disabled={value.trim() === '' && files.length === 0}
            onClick={submit}
          >
            <SendIcon />
          </Button>
        )}
      </div>
    </div>
  );
}
