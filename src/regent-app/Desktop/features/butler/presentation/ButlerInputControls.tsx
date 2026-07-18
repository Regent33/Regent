'use client';

import { useLayoutEffect, useRef, useState, type FormEvent, type KeyboardEvent } from 'react';
import gsap from 'gsap';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { KeyboardIcon, MicIcon, MicOffIcon, SendIcon } from '@/shared/ui/icons';

export function ButlerInputControls({
  micMuted,
  onToggleMic,
  onSubmit,
  disabled = false,
}: {
  micMuted: boolean;
  onToggleMic: () => void;
  onSubmit: (text: string) => void;
  disabled?: boolean;
}) {
  const s = t().butler;
  const [open, setOpen] = useState(false);
  const [value, setValue] = useState('');
  const composerRef = useRef<HTMLFormElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // Spring-like entrance uses compositor-only transforms/opacity. No width or
  // layout animation, so Butler's WebGL/canvas surfaces keep their frame rate.
  useLayoutEffect(() => {
    const composer = composerRef.current;
    if (!composer) return;
    const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
    gsap.killTweensOf(composer);
    if (reduced) {
      gsap.set(composer, { autoAlpha: open ? 1 : 0, y: 0, scale: 1 });
    } else if (open) {
      gsap.fromTo(
        composer,
        { autoAlpha: 0, y: 18, scaleX: 0.86, scaleY: 0.92 },
        { autoAlpha: 1, y: 0, scaleX: 1, scaleY: 1, duration: 0.5, ease: 'back.out(1.7)' },
      );
    } else {
      gsap.to(composer, { autoAlpha: 0, y: 10, scale: 0.94, duration: 0.2, ease: 'power2.in' });
    }
    if (open) requestAnimationFrame(() => inputRef.current?.focus());
    return () => void gsap.killTweensOf(composer);
  }, [open]);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const text = value.trim();
    if (text === '' || disabled) return;
    onSubmit(text);
    setValue('');
    setOpen(false);
  };

  const inputKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      setOpen(false);
    }
  };

  return (
    <div className="relative mt-3 flex items-center justify-center gap-2">
      <div
        className={`absolute bottom-[calc(100%+12px)] left-1/2 w-[min(560px,calc(100vw-32px))] -translate-x-1/2 ${
          open ? 'pointer-events-auto' : 'pointer-events-none'
        }`}
      >
        <form
          ref={composerRef}
          aria-hidden={!open}
          onSubmit={submit}
          className="flex w-full items-center gap-2 rounded-[18px] border border-stroke-secondary bg-surface/95 p-2 opacity-0 shadow-[var(--shadow-prompt)] backdrop-blur-xl"
        >
          <KeyboardIcon className="ml-2 size-4 shrink-0 text-text-tertiary" />
          <input
            ref={inputRef}
            value={value}
            tabIndex={open ? 0 : -1}
            disabled={disabled}
            maxLength={8_000}
            autoComplete="off"
            aria-label={s.keyboardPlaceholder}
            placeholder={s.keyboardPlaceholder}
            onKeyDown={inputKeyDown}
            onChange={(event) => setValue(event.target.value)}
            className="min-w-0 flex-1 bg-transparent px-1 py-2 text-sm text-text-primary outline-none placeholder:text-text-tertiary"
          />
          <Button
            type="submit"
            size="iconSm"
            aria-label={s.keyboardSend}
            title={s.keyboardSend}
            disabled={disabled || value.trim() === ''}
            className="rounded-full"
          >
            <SendIcon className="size-4" />
          </Button>
        </form>
      </div>

      <Button
        variant={micMuted ? 'secondary' : 'ghost'}
        size="icon"
        aria-pressed={micMuted}
        aria-label={micMuted ? s.micMuted : s.micOn}
        title={micMuted ? s.micMuted : s.micOn}
        onClick={onToggleMic}
        className={`size-10 rounded-full border bg-surface/80 shadow-[var(--shadow-prompt)] backdrop-blur-lg ${
          micMuted
            ? 'border-stroke-secondary text-text-tertiary'
            : 'border-accent text-accent'
        }`}
      >
        {micMuted ? <MicOffIcon className="size-[18px]" /> : <MicIcon className="size-[18px]" />}
      </Button>
      <Button
        variant={open ? 'secondary' : 'ghost'}
        size="icon"
        aria-expanded={open}
        aria-label={open ? s.keyboardClose : s.keyboardOpen}
        title={open ? s.keyboardClose : s.keyboardOpen}
        onClick={() => setOpen((shown) => !shown)}
        className="size-10 rounded-full border border-stroke-secondary bg-surface/80 shadow-[var(--shadow-prompt)] backdrop-blur-lg"
      >
        <KeyboardIcon className="size-[18px]" />
      </Button>
    </div>
  );
}
