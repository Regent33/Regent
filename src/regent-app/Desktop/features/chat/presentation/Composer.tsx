'use client';
// The chat composer — a floating rounded surface (borderless + shadow, like
// Hermes'): attach · auto-growing textarea · mic · model pill · circular
// send/stop (+ elapsed time while a turn runs). `/` at the start of an
// otherwise-empty line pops a command-completion menu; ↑/↓ on an
// empty/unedited composer cycles this session's prompt history.
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { MicIcon, PaperclipIcon, SendIcon, StopIcon } from '@/shared/ui/icons';
import { useTurnActivity } from '@/shared/state/deaconBus';
import { useInputHistory } from '@/features/chat/viewmodels/useInputHistory';
import { useSlashMenu } from '@/features/chat/viewmodels/useSlashMenu';
import { useElapsedSeconds } from '@/features/chat/viewmodels/useElapsedSeconds';
import { useSpeechToText } from '@/features/chat/viewmodels/useSpeechToText';
import { ModelPill } from '@/features/chat/presentation/composer/ModelPill';
import { PromptInputBar } from '@/features/chat/presentation/composer/PromptInputBar';
import { SlashMenu } from '@/features/chat/presentation/composer/SlashMenu';

export interface ComposerProps {
  busy: boolean;
  sessionId: string | undefined;
  onSubmit: (text: string, attachments?: readonly File[]) => void;
  onStop: () => void;
}

const MAX_ATTACH_BYTES = 20 * 1024 * 1024; // mirrors the deacon's decoded cap

function formatElapsed(totalSeconds: number): string {
  if (totalSeconds < 60) return `${totalSeconds}s`;
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${String(seconds).padStart(2, '0')}`;
}

export function Composer({ busy, sessionId, onSubmit, onStop }: ComposerProps) {
  const s = t().chat.composer;
  const [value, setValue] = useState('');
  const [files, setFiles] = useState<readonly File[]>([]);
  const [attachError, setAttachError] = useState<string>();
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const valueRef = useRef('');
  const speechBaseRef = useRef<string | undefined>(undefined);
  const history = useInputHistory();

  const setText = useCallback((next: string) => {
    valueRef.current = next;
    setValue(next);
  }, []);

  useEffect(() => {
    valueRef.current = value;
  }, [value]);

  const mergeSpeechText = useCallback((base: string, spoken: string) => {
    if (spoken.trim() === '') return base;
    return `${base}${base.trim() === '' || /\s$/.test(base) ? '' : ' '}${spoken}`;
  }, []);

  const speechCallbacks = useMemo(
    () => ({
      onStart: () => {
        speechBaseRef.current = valueRef.current;
      },
      onPreview: (spoken: string) => {
        const base = speechBaseRef.current ?? valueRef.current;
        setText(mergeSpeechText(base, spoken));
      },
      onFinal: (spoken: string) => {
        const base = speechBaseRef.current ?? valueRef.current;
        speechBaseRef.current = undefined;
        setText(mergeSpeechText(base, spoken));
        textareaRef.current?.focus();
      },
      onCancel: () => {
        if (speechBaseRef.current !== undefined) setText(speechBaseRef.current);
        speechBaseRef.current = undefined;
      },
    }),
    [mergeSpeechText, setText],
  );

  const speech = useSpeechToText(speechCallbacks);
  const slash = useSlashMenu(value, setText, () => textareaRef.current?.focus());

  const elapsed = useElapsedSeconds(useTurnActivity(sessionId) === 'running');
  const micLabel =
    speech.state === 'recording'
      ? s.micStop
      : speech.state === 'transcribing'
        ? s.micTranscribing
        : speech.state === 'starting'
          ? s.micStarting
          : s.mic;

  const submit = () => {
    const text = valueRef.current.trim();
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

      {(files.length > 0 || attachError !== undefined || speech.error !== undefined) && (
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
          {speech.error !== undefined && (
            <button
              type="button"
              className="text-left text-xs text-danger"
              onClick={speech.clearError}
              title={s.micError}
            >
              {speech.error}
            </button>
          )}
        </div>
      )}

      <PromptInputBar
        value={value}
        onChange={setText}
        onKeyDown={onKeyDown}
        placeholder={s.placeholder}
        textareaRef={textareaRef}
        left={
          <>
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
          </>
        }
        right={
          <>
            <Button
              variant={speech.state === 'recording' ? 'default' : 'ghost'}
              size="icon"
              aria-label={micLabel}
              title={micLabel}
              disabled={busy || speech.state === 'starting' || speech.state === 'transcribing' || !speech.supported}
              className={speech.state === 'recording' ? 'motion-safe:animate-pulse' : ''}
              onClick={speech.toggle}
            >
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
          </>
        }
      />
    </div>
  );
}
