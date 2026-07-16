'use client';
// One chat surface: empty state = the home hero over the composer; once the
// first message lands it becomes the streaming transcript. Remounted (via
// `key`) when the session id changes, so state never leaks across sessions.
import { useEffect } from 'react';
import { t } from '@/shared/i18n/t';
import { setActiveSession } from '@/shared/state/activeSession';
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
    <div className="flex h-full flex-col items-center justify-center gap-0 text-center">
      {/* Gradient rises with the letters: a deeper teal at the base up to the
          full accent. Deep→accent, never accent→white — mixing toward white
          desaturates to a chalky pastel that reads dirty on bone. py/-my
          extend the paint box without moving the layout — with leading this
          tight, glyph ink can poke outside the background box, and
          bg-clip-text turns whatever it misses invisible. */}
      <h1
        className="-my-3 bg-linear-to-t from-[color-mix(in_srgb,var(--accent),black_22%)] to-accent bg-clip-text py-3 text-7xl font-bold leading-[0.74] text-transparent md:text-9xl"
        style={{ fontFamily: 'var(--font-display)' }}
      >
        {strings.home.wordmark}
      </h1>
      <p className="mt-1 text-xl font-light text-text-secondary">
        {strings.home.pitch}
      </p>
    </div>
  );
}

export function ChatView({ sessionId }: { sessionId?: string }) {
  const { state, resuming, sessionId: liveSessionId, submit, stop, respondApproval } = useChatSession(sessionId);
  const { ref: scrollRef, atBottom, scrollToBottom } = useAutoScroll<HTMLDivElement>();

  // Publish the shown session to the titlebar's session menu.
  useEffect(() => {
    setActiveSession(liveSessionId);
    return () => setActiveSession(undefined);
  }, [liveSessionId]);

  return (
    <div className="relative flex h-full flex-col">
      {state.items.length > 0 && <Watermark />}
      {/* The composer floats OVER the transcript (absolute, below) so chat
          content extends and scrolls behind it. Composer clearance is the
          Transcript's own bottom sentinel (bottomClearance below) — padding
          on THIS scroll container doesn't work: Chromium excludes a scroll
          container's bottom padding from the scrollable extent of overflowing
          content, so a full scroll still buried the last message under the
          composer. */}
      <div ref={scrollRef} className="relative min-h-0 flex-1 overflow-y-auto">
        {resuming && state.items.length === 0 ? (
          <div className="flex h-full items-center justify-center">
            <Loader />
          </div>
        ) : state.items.length === 0 ? (
          <Hero />
        ) : (
          <Transcript
            items={state.items}
            busy={state.busy}
            onApproval={respondApproval}
            stickToBottom={atBottom}
            bottomClearance="h-[8.5rem]"
          />
        )}
      </div>
      {/* Sibling of the scroll container (NOT inside it) — an abspos child of a
          scrolling element scrolls away with the content; here it stays pinned
          just above the floating composer. */}
      {!atBottom && state.items.length > 0 && (
        <ScrollToBottomButton onClick={scrollToBottom} className="bottom-34" />
      )}
      <div className="absolute inset-x-0 bottom-6">
        <Composer busy={state.busy} sessionId={liveSessionId} onSubmit={submit} onStop={stop} />
      </div>
    </div>
  );
}
