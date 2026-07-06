'use client';
// Butler / Presenter Mode — the full-screen "Jarvis" call view: grid field,
// the braille voice mark, live captions, and the floating-window cluster
// (Conversation · Map · Insights). Esc or the corner X exits (the call tears
// down with it — mic off, loop disconnected).
import { useEffect, type ReactNode } from 'react';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { ErrorState } from '@/shared/ui/ErrorState';
import { CloseIcon } from '@/shared/ui/icons';
import { GridBackground } from '@/features/butler/presentation/GridBackground';
import { VoiceDots } from '@/features/butler/presentation/VoiceDots';
import { FloatingWindow } from '@/features/butler/presentation/FloatingWindow';
import { MapWindow } from '@/features/butler/presentation/MapWindow';
import { InsightsWindow } from '@/features/butler/presentation/InsightsWindow';
import { useButlerCall } from '@/features/butler/viewmodels/useButlerCall';
import { useWindows } from '@/features/butler/viewmodels/useWindows';
import type { CaptionEntry } from '@/features/butler/domain/phase';

const WINDOW_IDS = ['conversation', 'map', 'insights'] as const;

function ConversationLog({ log }: { log: readonly CaptionEntry[] }) {
  const s = t().butler.windows;
  if (log.length === 0) return <p className="text-xs text-text-tertiary">{s.conversationEmpty}</p>;
  return (
    <div className="flex flex-col gap-2">
      {log.map((entry, i) => (
        <p key={i} className="text-xs leading-relaxed">
          <span className="font-semibold text-text-tertiary">
            {entry.who === 'you' ? s.you : t().home.wordmark}
          </span>{' '}
          <span className="whitespace-pre-wrap break-words text-text-secondary">{entry.text}</span>
        </p>
      ))}
    </div>
  );
}

export function ButlerView({ onClose }: { onClose: () => void }) {
  const s = t().butler;
  const { state, analyserRef } = useButlerCall();
  const { windows, toggle, focus, move } = useWindows(WINDOW_IDS);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  const defs: Record<string, { title: string; width: number; content: ReactNode }> = {
    conversation: { title: s.windows.conversation, width: 300, content: <ConversationLog log={state.log} /> },
    map: { title: s.windows.map, width: 380, content: <MapWindow /> },
    insights: { title: s.windows.insights, width: 300, content: <InsightsWindow /> },
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={s.title}
      className="fixed inset-0 z-40 flex flex-col bg-bg motion-safe:animate-[fadeIn_200ms_ease-out]"
    >
      <GridBackground />
      <div className="relative flex items-center justify-between p-2">
        <div className="flex gap-1">
          {windows.map((w) => (
            <Button
              key={w.id}
              variant={w.open ? 'secondary' : 'ghost'}
              size="sm"
              onClick={() => toggle(w.id)}
            >
              {defs[w.id].title}
            </Button>
          ))}
        </div>
        <Button variant="ghost" size="icon" aria-label={s.close} title={s.close} onClick={onClose}>
          <CloseIcon />
        </Button>
      </div>
      {windows
        .filter((w) => w.open)
        .map((w) => (
          <FloatingWindow
            key={w.id}
            title={defs[w.id].title}
            closeLabel={s.windows.closeWindow}
            x={w.x}
            y={w.y}
            z={w.z}
            width={defs[w.id].width}
            onFocus={() => focus(w.id)}
            onClose={() => toggle(w.id)}
            onMove={(x, y) => move(w.id, x, y)}
          >
            {defs[w.id].content}
          </FloatingWindow>
        ))}
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
