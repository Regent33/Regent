'use client';
// One chat surface: empty state = the home hero over the composer; once the
// first message lands it becomes the streaming transcript. Remounted (via
// `key`) when the session id changes, so state never leaks across sessions.
import { useEffect } from 'react';
import { t } from '@/shared/i18n/t';
import { useRouter } from '@/shared/infrastructure/router/adapter';
import { setActiveSession } from '@/shared/state/activeSession';
import { Loader } from '@/shared/ui/Loader';
import { Watermark } from '@/shared/ui/Watermark';
import { ScrollToBottomButton } from '@/shared/ui/ScrollToBottomButton';
import { Composer } from '@/features/chat/presentation/Composer';
import { Transcript } from '@/shared/ui/Transcript';
import { useChatSession } from '@/features/chat/viewmodels/useChatSession';
import { useAutoScroll } from '@/features/chat/viewmodels/useAutoScroll';

const CODE_TASK_ACTION =
  /\b(add|build|change|compile|connect|create|debug|delete|deploy|fix|generate|implement|install|lessen|lint|make|modify|patch|refactor|remove|rename|run|scaffold|test|typecheck|update|wire)\b/i;
const CODE_TASK_TARGET =
  /\b(api|app|bug|button|cargo|code|component|compiler|css|database|endpoint|error|file|function|hook|lint|npm|page|repo|rust|screen|search bar|session|style|test|tsx?|ui|viewmodel)\b|(?:^|\s)[\w./\\-]+\.(?:css|html|js|jsx|json|md|rs|ts|tsx|toml|yaml|yml)\b/i;
const EXPLAIN_ONLY = /^(?:can you\s+)?(?:explain|how do i|how does|tell me|what is|what are|why does|why is)\b/i;

function isCodingTaskPrompt(text: string): boolean {
  const prompt = text.trim();
  if (prompt === '' || prompt.startsWith('/')) return false;
  if (EXPLAIN_ONLY.test(prompt) && !CODE_TASK_ACTION.test(prompt)) return false;
  return CODE_TASK_ACTION.test(prompt) && CODE_TASK_TARGET.test(prompt);
}

function Hero() {
  const strings = t();
  return (
    <div className="flex h-full flex-col items-center justify-center gap-0 text-center">
      <h1
        className="text-6xl font-bold leading-[0.74] text-accent md:text-8xl"
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
  const router = useRouter();

  // Publish the shown session to the titlebar's session menu.
  useEffect(() => {
    setActiveSession(liveSessionId);
    return () => setActiveSession(undefined);
  }, [liveSessionId]);

  const submitOrRedirect = (text: string, attachments?: readonly File[]) => {
    if ((attachments?.length ?? 0) === 0 && isCodingTaskPrompt(text)) {
      router.push(`/code?task=${encodeURIComponent(text)}`);
      return;
    }
    submit(text, attachments);
  };

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
        <Composer busy={state.busy} sessionId={liveSessionId} onSubmit={submitOrRedirect} onStop={stop} />
      </div>
    </div>
  );
}
