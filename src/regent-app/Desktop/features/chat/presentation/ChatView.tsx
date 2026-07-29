'use client';
// One chat surface: empty state = the home hero over the composer; once the
// first message lands it becomes the streaming transcript. Remounted (via
// `key`) when the session id changes, so state never leaks across sessions.
import { useEffect, useMemo, useRef, useState } from 'react';
import { t } from '@/shared/i18n/t';
import { setActiveSession } from '@/shared/state/activeSession';
import { deaconRequest } from '@/shared/infrastructure/rpc/client';
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
import { WorkspacePanel } from '@/features/workspace/presentation/WorkspacePanel';
import { WorktreeIcon } from '@/shared/ui/icons';

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
  const {
    state,
    resuming,
    sessionId: liveSessionId,
    submit,
    stop,
    respondApproval,
    ensureSession,
  } = useChatSession(sessionId);
  // Collapsed by default — the panel is for coding sessions, and chat should
  // look unchanged until it's asked for.
  const [panelOpen, setPanelOpen] = useState(false);
  // Maximized hides the chat column outright — "full screen" that leaves a
  // squeezed ribbon of chat behind isn't full screen.
  const [panelMaximized, setPanelMaximized] = useState(false);
  // Bumped on a workspace rebind. The panel keys off it so root, tree and
  // git status all refetch — the session id is unchanged, so nothing else
  // would tell them the tree moved.
  const [workspaceEpoch, setWorkspaceEpoch] = useState(0);

  // Opening a folder attaches it to THIS conversation.
  //
  // It used to start a new chat and navigate there whenever the session already
  // existed, because the workspace was fixed at birth and there was nowhere to
  // put a folder picked mid-conversation. Reported 2026-07-29: you pick a repo
  // and the conversation you were in disappears. `workspace.set` rebinds the
  // live session instead — the deacon recomputes the sandbox jail exactly as it
  // would at birth, so opening a real repo still jails the session to it.
  const openWorkspace = async (path: string) => {
    if (liveSessionId === undefined) {
      await ensureSession(path);
      return;
    }
    const bound = await deaconRequest('workspace.set', {
      session_id: liveSessionId,
      root: path,
    });
    if (bound.ok) setWorkspaceEpoch((n) => n + 1);
  };
  const activity = useTurnActivity(liveSessionId);
  const turnError = useTurnError(liveSessionId);
  const busy = chatBusy(state.busy, activity);
  const items = withTurnError(state.items, turnError);
  const { ref: scrollRef, atBottom, scrollToBottom } = useAutoScroll<HTMLDivElement>();
  // Every file path this conversation's tools touched — already on the tool
  // rows (live and resumed alike), so the panel can scope the shared sandbox
  // to this session's own folders without any extra backend state.
  const touchedPaths = useMemo(
    () =>
      items.flatMap((item) =>
        item.kind === 'tool' && item.detail !== undefined ? [item.detail] : [],
      ),
    [items],
  );

  // A submit while busy is queued (not dropped) and flushed FIFO once the
  // turn ends — Composer still calls onSubmit on Enter while busy (Send
  // itself is hidden then, replaced by Stop); this decides queue vs send.
  // The count is shown ONLY as a quiet composer-side label (queuedCount
  // below) — it must never touch the transcript/reducer: dispatching a
  // notice there seals the in-flight streaming reply (sealStreaming) and
  // makes the CURRENT turn look like it restarted/stalled, which is exactly
  // the "stuck on thinking" regression this avoids.
  const queue = useRef(createPromptQueue());
  const [queuedCount, setQueuedCount] = useState(0);
  const onSubmit = (text: string, attachments?: readonly File[]) => {
    const position = enqueueIfBusy(queue.current, busy, { text, attachments });
    if (position !== undefined) {
      setQueuedCount(position);
      return;
    }
    submit(text, attachments);
  };
  useEffect(() => {
    const next = dequeueOnBusyEnd(queue.current, busy);
    if (next !== undefined) {
      setQueuedCount(queue.current.items.length);
      submit(next.text, next.attachments);
    }
  }, [busy, submit]);

  // Publish the shown session to the titlebar's session menu.
  useEffect(() => {
    setActiveSession(liveSessionId);
    return () => setActiveSession(undefined);
  }, [liveSessionId]);

  return (
    <div className="flex h-full">
      <div
        className={`relative h-full min-w-0 flex-1 flex-col ${
          panelOpen && panelMaximized ? 'hidden' : 'flex'
        }`}
      >
      {items.length > 0 && <Watermark />}
      {/* Panel toggle: a quiet affordance pinned top-right of the chat column,
          so the surface looks unchanged until someone reaches for it. Hidden
          once the panel is open — it sat directly on the panel's left edge and
          collided with it; closing lives in the panel header instead. */}
      {!panelOpen && (
      <button
        type="button"
        aria-label={t().workspace.open}
        title={t().workspace.open}
        className="absolute right-3 top-3 z-10 flex cursor-pointer items-center gap-1.5 rounded-[4px] px-2 py-1 text-[11px] text-text-tertiary hover:bg-hover hover:text-text-primary"
        onClick={() => setPanelOpen((o) => !o)}
      >
        <WorktreeIcon className="size-3.5" />
        {t().workspace.title}
      </button>
      )}
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
        <Composer
          busy={busy}
          sessionId={liveSessionId}
          onSubmit={onSubmit}
          onStop={stop}
          queuedCount={queuedCount}
        />
      </div>
      </div>
      {panelOpen && (
        <WorkspacePanel
          key={workspaceEpoch}
          sessionId={liveSessionId}
          busy={busy}
          touchedPaths={touchedPaths}
          onOpenFolder={openWorkspace}
          maximized={panelMaximized}
          onToggleMaximize={() => setPanelMaximized((m) => !m)}
          onClose={() => {
            setPanelOpen(false);
            setPanelMaximized(false);
          }}
        />
      )}
    </div>
  );
}
