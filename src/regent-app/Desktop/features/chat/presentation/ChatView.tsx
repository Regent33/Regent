'use client';
// One chat surface: empty state = the home hero over the composer; once the
// first message lands it becomes the streaming transcript. Remounted (via
// `key`) when the session id changes, so state never leaks across sessions.
import { t } from '@/shared/i18n/t';
import { Loader } from '@/shared/ui/Loader';
import { Watermark } from '@/shared/ui/Watermark';
import { ScrollToBottomButton } from '@/shared/ui/ScrollToBottomButton';
import { Composer } from '@/features/chat/presentation/Composer';
import { Transcript } from '@/shared/ui/Transcript';
import { useChatSession } from '@/features/chat/viewmodels/useChatSession';
import { useAutoScroll } from '@/features/chat/viewmodels/useAutoScroll';

function Hero() {
  const strings = t();
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 text-center">
      <h1
        className="text-6xl font-bold leading-none text-accent md:text-8xl"
        style={{ fontFamily: 'var(--font-display)' }}
      >
        {strings.home.wordmark}
      </h1>
      <p className="text-lg text-text-secondary">{strings.home.pitch}</p>
    </div>
  );
}

export function ChatView({ sessionId }: { sessionId?: string }) {
  const { state, resuming, sessionId: liveSessionId, submit, stop, respondApproval } = useChatSession(sessionId);
  const { ref: scrollRef, atBottom, scrollToBottom } = useAutoScroll<HTMLDivElement>();

  return (
    <div className="relative flex h-full flex-col">
      {state.items.length > 0 && <Watermark />}
      {/* The composer floats OVER the transcript (absolute, below) so chat
          content extends and scrolls behind it; the bottom padding keeps the
          last message reachable above the pill. */}
      {/* pb-28 clears the floating composer for transcript content; the empty
          hero drops it so the wordmark truly centers in the pane. */}
      <div
        ref={scrollRef}
        className={`relative min-h-0 flex-1 overflow-y-auto ${state.items.length > 0 ? 'pb-28' : ''}`}
      >
        {resuming && state.items.length === 0 ? (
          <div className="flex h-full items-center justify-center">
            <Loader />
          </div>
        ) : state.items.length === 0 ? (
          <Hero />
        ) : (
          <Transcript items={state.items} busy={state.busy} onApproval={respondApproval} stickToBottom={atBottom} />
        )}
        {!atBottom && state.items.length > 0 && <ScrollToBottomButton onClick={scrollToBottom} />}
      </div>
      <div className="absolute inset-x-0 bottom-0">
        <Composer busy={state.busy} sessionId={liveSessionId} onSubmit={submit} onStop={stop} />
      </div>
    </div>
  );
}
