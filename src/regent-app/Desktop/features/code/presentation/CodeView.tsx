'use client';
// The Code surface — regent-code's plan → approve → run → verify/revert flow.
// The run log is the shared Transcript (deltas, tool rows, approval cards).
import { useCallback, useEffect, useRef } from 'react';
import { useRouter, useSearchParams } from '@/shared/infrastructure/router/adapter';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { ErrorState } from '@/shared/ui/ErrorState';
import { Loader } from '@/shared/ui/Loader';
import { Markdown } from '@/shared/ui/Markdown';
import { ScrollToBottomButton } from '@/shared/ui/ScrollToBottomButton';
import { Transcript } from '@/shared/ui/Transcript';
import { useCodeRun } from '@/features/code/viewmodels/useCodeRun';
import { useAutoScroll } from '@/features/chat/viewmodels/useAutoScroll';
import { Composer } from '@/features/chat/presentation/Composer';

export function CodeView() {
  const s = t().code;
  const router = useRouter();
  const params = useSearchParams();
  const run = useCodeRun();
  // Same scroll contract as ChatView: auto-follow only while at the bottom
  // (reading the run log mid-stream is never yanked down), with the floating
  // return-to-latest button once scrolled away.
  const { ref: scrollRef, atBottom, scrollToBottom } = useAutoScroll<HTMLDivElement>();

  const consumedTaskRef = useRef<string | undefined>(undefined);
  const idle = run.phase === 'idle' || run.phase === 'planning';
  const initialTask = params.get('task')?.trim() ?? '';

  const makePlan = useCallback((text: string) => {
    const task = text.trim();
    if (task === '' || run.phase === 'planning') return;
    run.makePlan(task);
  }, [run.makePlan, run.phase]);

  useEffect(() => {
    if (initialTask === '' || consumedTaskRef.current === initialTask || run.phase !== 'idle') return;
    consumedTaskRef.current = initialTask;
    // Not seeded into the composer: the running task echoes above the plan
    // (chat-style), and text lingering in the input read as "not sent".
    run.makePlan(initialTask);
    router.replace('/code');
  }, [initialTask, router, run.makePlan, run.phase]);

  return (
    <div className="mx-auto flex h-full max-w-205 flex-col gap-4 px-6 py-6">
      {!idle && <h1 className="text-lg font-semibold text-text-primary">{s.title}</h1>}

      {/* The task echoes the moment of submit (chat-style: your message, then
          pending dots while the plan streams in) — never dead air. */}
      {run.phase !== 'idle' && (
        <p className="whitespace-pre-wrap text-sm text-text-secondary">{run.task}</p>
      )}
      {run.phase === 'planning' && (
        <div className="motion-safe:animate-[fadeIn_150ms_ease-out]">
          <Loader />
        </div>
      )}

      {run.error !== undefined && <ErrorState compact description={run.error} />}

      {run.phase === 'plan-ready' && (
        <div className="flex min-h-0 flex-col gap-3">
          <div
            className="max-h-[45vh] overflow-y-auto rounded-md bg-bg p-4"
            style={{ boxShadow: 'var(--shadow-elev)' }}
          >
            <Markdown text={run.plan} />
          </div>
          <div className="flex gap-2 self-end">
            <Button onClick={run.approveRun}>{s.approveRun}</Button>
            <Button variant="secondary" onClick={run.discard}>
              {s.discard}
            </Button>
          </div>
        </div>
      )}

      {(run.phase === 'running' || run.phase === 'done') && (
        <div className="relative flex min-h-0 flex-1 flex-col gap-3">
          {run.phase === 'running' && (
            <div className="flex items-center gap-3">
              <Loader />
              <span className="text-xs uppercase tracking-widest text-text-tertiary">{s.running}</span>
              <Button variant="text" size="sm" className="ml-auto" onClick={run.stop}>
                {t().chat.composer.stop}
              </Button>
            </div>
          )}
          <div ref={scrollRef} className="relative min-h-0 flex-1 overflow-y-auto">
            <Transcript
              items={run.log.items}
              onApproval={run.respondApproval}
              stickToBottom={atBottom}
            />
          </div>
          {/* Sibling of the scroll container (an abspos child of a scrolling
              element scrolls away with the content). */}
          {!atBottom && <ScrollToBottomButton onClick={scrollToBottom} />}
          {run.phase === 'done' && (
            <div className="flex flex-col gap-2 rounded-md bg-bg p-4" style={{ boxShadow: 'var(--shadow-elev)' }}>
              {run.verify !== undefined && (
                <p className={`text-sm font-semibold ${run.verify.passed ? 'text-accent' : 'text-danger'}`}>
                  {run.verify.passed ? s.verifyPassed : s.verifyFailed}
                  {run.verify.summary !== '' && (
                    <span className="ml-2 font-normal text-text-secondary">{run.verify.summary}</span>
                  )}
                </p>
              )}
              {run.reverted && <p className="text-sm text-danger">{s.reverted}</p>}
              {run.report !== undefined && <Markdown text={run.report} />}
              <Button variant="secondary" size="sm" className="self-end" onClick={run.discard}>
                {s.done}
              </Button>
            </div>
          )}
        </div>
      )}

      {idle && (
        <div className="relative min-h-0 flex-1">
          <div className="pointer-events-none absolute inset-0 flex items-center justify-center text-center">
            <h1
              className="text-5xl font-bold leading-[0.74] text-accent md:text-7xl"
              style={{ fontFamily: 'var(--font-display)' }}
            >
              {s.heroTitle}
            </h1>
          </div>
          <div className="absolute inset-x-0 bottom-0">
            {/* Clears on submit like chat — the task echoes above the plan the
                moment it's sent, so keeping it in the input read as "not sent". */}
            <Composer
              busy={run.phase === 'planning'}
              sessionId={undefined}
              onSubmit={makePlan}
              onStop={run.stop}
              placeholder={s.taskPlaceholder}
            />
          </div>
        </div>
      )}
    </div>
  );
}
