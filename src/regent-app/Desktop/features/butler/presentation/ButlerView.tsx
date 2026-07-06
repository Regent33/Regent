'use client';
// Butler / Presenter Mode — the full-screen "Jarvis" call view: grid field,
// kinetic particle core, and live captions. Esc or the corner X exits (the
// call tears down with it — mic off, loop disconnected).
import { useEffect } from 'react';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { ErrorState } from '@/shared/ui/ErrorState';
import { CloseIcon } from '@/shared/ui/icons';
import { GridBackground } from '@/features/butler/presentation/GridBackground';
import { VoiceDots } from '@/features/butler/presentation/VoiceDots';
import { useButlerCall } from '@/features/butler/viewmodels/useButlerCall';

export function ButlerView({ onClose }: { onClose: () => void }) {
  const s = t().butler;
  const { state, analyserRef } = useButlerCall();

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  return (
    <div role="dialog" aria-modal="true" aria-label={s.title} className="fixed inset-0 z-40 flex flex-col bg-bg">
      <GridBackground />
      <div className="relative flex justify-end p-2">
        <Button variant="ghost" size="icon" aria-label={s.close} title={s.close} onClick={onClose}>
          <CloseIcon />
        </Button>
      </div>
      <div className="relative m-auto flex items-center justify-center">
        <VoiceDots analyserRef={analyserRef} speaking={state.phase === 'speaking'} scale={1.8} />
      </div>
      <div className="relative mx-auto mb-10 flex w-full max-w-[640px] flex-col items-center gap-1.5 px-6 text-center">
        {state.error !== null && state.phase === 'connecting' ? (
          // Setup failure (server/mic) — the call never started; say why, loudly.
          <ErrorState description={state.error} />
        ) : (
          <>
            <p className="text-[11px] font-semibold uppercase tracking-[0.14em] text-text-tertiary">
              {s.phases[state.phase]}
            </p>
            {state.heard !== '' && <p className="text-sm text-text-tertiary">{state.heard}</p>}
            {state.reply !== '' && <p className="line-clamp-3 text-sm text-text-secondary">{state.reply}</p>}
            {state.error !== null && <ErrorState compact description={state.error} />}
          </>
        )}
      </div>
    </div>
  );
}
