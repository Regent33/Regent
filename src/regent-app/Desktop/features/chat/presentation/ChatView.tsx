'use client';
// One chat surface: empty state = the home hero over the composer; once the
// first message lands it becomes the streaming transcript. Remounted (via
// `key`) when the session id changes, so state never leaks across sessions.
import { useEffect, useRef } from 'react';
import { t } from '@/shared/i18n/t';
import { setActiveSession } from '@/shared/state/activeSession';
import { useTurnActivity, useTurnError } from '@/shared/state/deaconBus';
import { Loader } from '@/shared/ui/Loader';
import { Watermark } from '@/shared/ui/Watermark';
import { ScrollToBottomButton } from '@/shared/ui/ScrollToBottomButton';
import { Composer } from '@/features/chat/presentation/Composer';
import { chatBusy, withTurnError } from '@/features/chat/presentation/chatDisplayState';
import { createPromptQueue, dequeueOnBusyEnd, enqueueIfBusy } from '@/features/chat/domain/promptQueue';
import { Transcript } from '@/shared/ui/Transcript';
import { useChatSession } from '@/features/chat/viewmodels/useChatSession';
import { useAutoScroll } from '@/features/chat/viewmodels/useAutoScroll';

function Hero() {
  const strings = t();
  return (
    <div className="flex h-full flex-col items-center justify-center gap-0 text-center">
      {/* The gradient's base comes from the theme (--wordmark-base): flat
          accent in light mode, a deepened teal rising to the accent in dark
          — see globals.css. py/-my extend the paint box without moving the
          layout — with leading this tight, glyph ink can poke outside the
          background box, and bg-clip-text turns whatever it misses invisible. */}
      <h1
        className="-my-3 bg-linear-to-t from-(--wordmark-base) to-accent bg-clip-text py-3 text-7xl font-bold leading-[0.74] text-transparent md:text-9xl"
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
  const { state, resuming, sessionId: liveSessionId, submit, stop, respondApproval, note } =
    useChatSession(sessionId);
  const activity = useTurnActivity(liveSessionId);
  const turnError = useTurnError(liveSessionId);
  const busy = chatBusy(state.busy, activity);
  const items = withTurnError(state.items, turnError);
  const { ref: scrollRef, atBottom, scrollToBottom } = useAutoScroll<HTMLDivElement>();

  // A submit while busy is queued (not dropped) and flushed FIFO once the
  // turn ends — Composer still calls onSubmit on Enter while busy (Send
  // itself is hidden then, replaced by Stop); this decides queue vs send.
  const queue = useRef(createPromptQueue());
  const onSubmit = (text: string, attachments?: readonly File[]) => {
    const position = enqueueIfBusy(queue.current, busy, { text, attachments });
    if (position !== undefined) {
      note(`⏳ Queued (${position}) — sends when the current turn finishes`);
      return;
    }
    submit(text, attachments);
  };
  useEffect(() => {
    const next = dequeueOnBusyEnd(queue.current, busy);
    if (next !== undefined) submit(next.text, next.attachments);
  }, [busy, submit]);

  // Publish the shown session to the titlebar's session menu.
  useEffect(() => {
    setActiveSession(liveSessionId);
    return () => setActiveSession(undefined);
  }, [liveSessionId]);

  return (
    <div className="relative flex h-full flex-col">
      {items.length > 0 && <Watermark />}
      {/* The composer floats OVER the transcript (absolute, below) so chat
          content extends and scrolls behind it. Composer clearance is the
          Transcript's own bottom sentinel (bottomClearance below) — padding
          on THIS scroll container doesn't work: Chromium excludes a scroll
          container's bottom padding from the scrollable extent of overflowing
          content, so a full scroll still buried the last message under the
          composer. */}
      <div ref={scrollRef} className="relative min-h-0 flex-1 overflow-y-auto">
        {resuming && items.length === 0 ? (
          <div className="flex h-full items-center justify-center">
            <Loader />
          </div>
        ) : items.length === 0 ? (
          <Hero />
        ) : (
          <Transcript
            items={items}
            busy={busy}
            onApproval={respondApproval}
            stickToBottom={atBottom}
            bottomClearance="h-[8.5rem]"
          />
        )}
      </div>
      {/* Sibling of the scroll container (NOT inside it) — an abspos child of a
          scrolling element scrolls away with the content; here it stays pinned
          just above the floating composer. */}
      {!atBottom && items.length > 0 && (
        <ScrollToBottomButton onClick={scrollToBottom} className="bottom-34" />
      )}
      <div className="absolute inset-x-0 bottom-6">
        <Composer busy={busy} sessionId={liveSessionId} onSubmit={onSubmit} onStop={stop} />
      </div>
    </div>
  );
}
