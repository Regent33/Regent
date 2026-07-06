'use client';
// Startup splash (Hermes-style: accent-bordered blank canvas, one quiet
// word) — shown until the deacon answers, then fades into the shell.
import { t } from '@/shared/i18n/t';

export function BootSplash({ done }: { done: boolean }) {
  return (
    <div
      aria-hidden={done}
      className={`fixed inset-0 z-[70] flex items-center justify-center border-4 border-accent bg-bg transition-opacity duration-500 ease-in ${
        done ? 'pointer-events-none opacity-0' : 'opacity-100'
      }`}
    >
      <p
        className="loader-dot text-sm font-semibold uppercase tracking-[0.4em] text-accent"
        style={{ fontFamily: 'var(--font-display)' }}
      >
        {t().splash.connecting}
      </p>
    </div>
  );
}
