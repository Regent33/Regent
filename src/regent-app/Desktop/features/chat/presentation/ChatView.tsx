'use client';
// One chat surface: empty state = the home hero over the composer; once the
// first message lands it becomes the streaming transcript. Remounted (via
// `key`) when the session id changes, so state never leaks across sessions.
import { t } from '@/shared/i18n/t';
import { Watermark } from '@/shared/ui/Watermark';
import { Composer } from '@/features/chat/presentation/Composer';
import { Transcript } from '@/features/chat/presentation/Transcript';
import { useChatSession } from '@/features/chat/viewmodels/useChatSession';

function Hero() {
  const strings = t();
  return (
    <div className="flex h-full flex-col items-center justify-center gap-4 text-center">
      <h1
        className="text-6xl font-bold text-accent md:text-8xl"
        style={{ fontFamily: 'var(--font-display)' }}
      >
        {strings.home.wordmark}
      </h1>
      <p className="text-lg text-text-secondary">{strings.home.pitch}</p>
    </div>
  );
}

export function ChatView({ sessionId }: { sessionId?: string }) {
  const { state, submit, stop } = useChatSession(sessionId);

  return (
    <div className="relative flex h-full flex-col">
      {state.items.length > 0 && <Watermark />}
      <div className="relative min-h-0 flex-1 overflow-y-auto">
        {state.items.length === 0 ? <Hero /> : <Transcript items={state.items} />}
      </div>
      <Composer busy={state.busy} onSubmit={submit} onStop={stop} />
    </div>
  );
}
