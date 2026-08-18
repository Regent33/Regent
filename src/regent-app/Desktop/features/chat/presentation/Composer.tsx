'use client';
// The chat composer — a floating rounded surface (borderless + shadow, like
// Hermes'): attach · auto-growing textarea · mic · model pill · circular
// send/stop (+ elapsed time while a turn runs). `/` at the start of an
// otherwise-empty line pops a command-completion menu; ↑/↓ on an
// empty/unedited composer cycles this session's prompt history.
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import {
  ButlerIcon,
  MicIcon,
  PaperclipIcon,
  SendIcon,
  StopIcon,
  WorktreeIcon,
} from '@/shared/ui/icons';
import { toggleButler } from '@/shared/state/butler';
import { useTurnActivity } from '@/shared/state/deaconBus';
import { clearEditorContext, setContextEnabled, useEditorContext } from '@/shared/state/openFile';
import { useFileDrop } from '@/features/chat/viewmodels/useFileDrop';
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
  /** `queueInstead` asks to wait behind the turn in flight instead of
   * interrupting it — sending mid-turn barges by default. */
  onSubmit: (text: string, attachments?: readonly File[], queueInstead?: boolean) => void;
  onStop: () => void;
  placeholder?: string;
  initialValue?: string;
  clearOnSubmit?: boolean;
  /** Messages queued behind the current turn — a quiet static count, never a
   * loading/thinking indicator (that belongs to the turn actually running). */
  queuedCount?: number;
}

const MAX_ATTACH_BYTES = 20 * 1024 * 1024; // mirrors the deacon's decoded cap

function formatElapsed(totalSeconds: number): string {
  if (totalSeconds < 60) return `${totalSeconds}s`;
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${String(seconds).padStart(2, '0')}`;
}

export function Composer({
  busy,
  sessionId,
  onSubmit,
  onStop,
  placeholder,
  initialValue,
  clearOnSubmit = true,
  queuedCount = 0,
}: ComposerProps) {
  const s = t().chat.composer;
  const inputPlaceholder = placeholder ?? s.placeholder;
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

  useEffect(() => {
    if (initialValue !== undefined) setText(initialValue);
  }, [initialValue, setText]);

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

  const editorContext = useEditorContext();
  const elapsed = useElapsedSeconds(useTurnActivity(sessionId) === 'running');
  const micLabel =
    speech.state === 'recording'
      ? s.micStop
      : speech.state === 'transcribing'
        ? s.micTranscribing
        : speech.state === 'starting'
          ? s.micStarting
          : s.mic;

  const submit = (queueInstead = false) => {
    const text = valueRef.current.trim();
    // A message needs text OR at least one attachment. While busy, onSubmit
    // still fires — the parent decides interrupt vs queue (see ChatView);
    // Send itself is hidden while busy (replaced by Stop), so only Enter
    // reaches here in that state.
    if (text === '' && files.length === 0) return;
    onSubmit(text, files.length > 0 ? files : undefined, queueInstead);
    if (text !== '') history.record(text);
    if (clearOnSubmit) setText('');
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

  // Dropping a file anywhere in the window attaches it, exactly as the
  // paperclip does — same size cap, same chip list, same send path.
  const dragging = useFileDrop(addFiles, !busy);

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (slash.onKeyDown(e)) return;
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      // Enter mid-turn INTERRUPTS — typing over an answer you no longer want
      // is the reflex, and it is what Butler does. Ctrl/Cmd+Enter queues
      // instead, for a follow-up meant to be handled after this one.
      submit(e.ctrlKey || e.metaKey);
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
      {/* Drop anywhere in the window; the hook owns the events, so this is
          purely the affordance and must not intercept the drop itself. */}
      {dragging && (
        <div className="pointer-events-none fixed inset-0 z-50 flex items-center justify-center bg-bg/70 motion-safe:animate-[fadeIn_100ms_ease-out]">
          <div className="rounded-xl border-2 border-dashed border-accent bg-surface px-6 py-4 text-sm text-text-secondary">
            {s.dropHint}
          </div>
        </div>
      )}

      {slash.open && (
        <SlashMenu
          items={slash.items}
          selected={slash.selected}
          onPick={slash.accept}
          onClose={slash.dismiss}
        />
      )}

      {/* Static text, no loader/animation — the turn already running owns the
          pending indicator. This is only a quiet acknowledgment that later
          messages were received. */}
      {queuedCount > 0 && (
        <div className="mb-1.5 px-1 text-xs text-text-tertiary">
          {queuedCount} {s.queued}
        </div>
      )}

      {/* What the agent will be told about, and the switch to stop telling it.
          Shown only when there IS something — an empty chip would just be
          furniture. */}
      {editorContext.path !== undefined || editorContext.folder !== undefined ? (
        // Two siblings, not a button inside a button: the chip body toggles
        // sharing, the × forgets the selection outright. The toggle alone was
        // not enough — a struck-through chip still sat above every composer,
        // and the only way to be rid of it was to reopen the panel and
        // deselect. Reported 2026-07-30.
        <div
          // `w-fit`: a div is block-level, so plain `flex` stretched the chip
          // across the whole composer. The <button> this replaced shrank to fit
          // for free (buttons default to width:fit-content) and losing that was
          // not intentional.
          className={`mb-1.5 flex w-fit max-w-full items-center gap-1 rounded-md pr-1 text-[11px] ${
            editorContext.enabled ? 'bg-hover' : 'opacity-60'
          }`}
        >
          <button
            type="button"
            aria-pressed={editorContext.enabled}
            title={editorContext.enabled ? s.contextOnHint : s.contextOffHint}
            className={`flex min-w-0 items-center gap-1.5 px-2 py-0.5 ${
              editorContext.enabled ? 'text-text-secondary' : 'text-text-tertiary line-through'
            }`}
            onClick={() => setContextEnabled(!editorContext.enabled)}
          >
            <WorktreeIcon className="size-3 shrink-0" />
            <span className="truncate">
              {editorContext.path ?? editorContext.folder}
              {editorContext.hasSelection && editorContext.path !== undefined
                ? ` · ${s.contextSelection}`
                : ''}
            </span>
          </button>
          <button
            type="button"
            aria-label={s.contextClear}
            title={s.contextClear}
            className="shrink-0 px-0.5 text-text-tertiary hover:text-text-primary"
            onClick={clearEditorContext}
          >
            ×
          </button>
        </div>
      ) : null}

      {(files.length > 0 || attachError !== undefined || speech.error !== undefined) && (
        <div className="mb-1.5 flex flex-wrap items-center gap-1.5 px-1">
          {files.map((file, i) => (
            // Capped and truncated: a long document name (they routinely run
            // 80+ chars) stretched this chip the full width of the composer.
            <span
              key={`${file.name}-${i}`}
              title={file.name}
              className="inline-flex max-w-[16rem] items-center gap-1 rounded-md bg-hover px-2 py-0.5 text-xs text-text-secondary"
            >
              <span className="truncate">{file.name}</span>
              <button
                type="button"
                aria-label={s.attachRemove}
                className="shrink-0 text-text-tertiary hover:text-text-primary"
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
        placeholder={inputPlaceholder}
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
              variant="ghost"
              size="icon"
              aria-label={t().shell.titlebar.butler}
              title={t().shell.titlebar.butler}
              onClick={toggleButler}
            >
              <ButlerIcon />
            </Button>

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
                // Wrapped: a bare `submit` would hand the click event in as
                // `barge`, and every mouse-sent message would interrupt.
                onClick={() => submit()}
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
