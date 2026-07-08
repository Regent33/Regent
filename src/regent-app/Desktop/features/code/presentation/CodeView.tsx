'use client';
// The Code surface — regent-code's plan → approve → run → verify/revert flow.
// The run log is the shared Transcript (deltas, tool rows, approval cards).
import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from 'react';
import { useRouter, useSearchParams } from 'next/navigation';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { ErrorState } from '@/shared/ui/ErrorState';
import { Loader } from '@/shared/ui/Loader';
import { Markdown } from '@/shared/ui/Markdown';
import { Transcript } from '@/shared/ui/Transcript';
import { SendIcon } from '@/shared/ui/icons';
import { useCodeRun } from '@/features/code/viewmodels/useCodeRun';
import { useSlashMenu } from '@/features/chat/viewmodels/useSlashMenu';
import { PromptInputBar } from '@/features/chat/presentation/composer/PromptInputBar';
import { SlashMenu } from '@/features/chat/presentation/composer/SlashMenu';

export function CodeView() {
  const s = t().code;
  const router = useRouter();
  const params = useSearchParams();
  const run = useCodeRun();
  const [draft, setDraft] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const consumedTaskRef = useRef<string | undefined>(undefined);
  const slash = useSlashMenu(draft, setDraft, () => textareaRef.current?.focus());
  const idle = run.phase === 'idle' || run.phase === 'planning';
  const initialTask = params.get('task')?.trim() ?? '';

  const makeDraftPlan = useCallback(() => {
    const task = draft.trim();
    if (task === '' || run.phase === 'planning') return;
    run.makePlan(task);
  }, [draft, run.makePlan, run.phase]);

  useEffect(() => {
    if (initialTask === '' || consumedTaskRef.current === initialTask || run.phase !== 'idle') return;
    consumedTaskRef.current = initialTask;
    setDraft(initialTask);
    run.makePlan(initialTask);
    router.replace('/code');
  }, [initialTask, router, run.makePlan, run.phase]);

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (slash.onKeyDown(e)) return;
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      makeDraftPlan();
    }
  };

  return (
    <div className="mx-auto flex h-full max-w-[820px] flex-col gap-4 px-6 py-6">
      {!idle && <h1 className="text-lg font-semibold text-text-primary">{s.title}</h1>}

      {run.phase !== 'idle' && run.phase !== 'planning' && (
        <p className="whitespace-pre-wrap text-sm text-text-secondary">{run.task}</p>
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
        <div className="flex min-h-0 flex-1 flex-col gap-3">
          {run.phase === 'running' && (
            <div className="flex items-center gap-3">
              <Loader />
              <span className="text-xs uppercase tracking-[0.1em] text-text-tertiary">{s.running}</span>
              <Button variant="text" size="sm" className="ml-auto" onClick={run.stop}>
                {t().chat.composer.stop}
              </Button>
            </div>
          )}
          <div className="min-h-0 flex-1 overflow-y-auto">
            <Transcript items={run.log.items} onApproval={run.respondApproval} />
          </div>
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
          <div className="absolute inset-x-0 bottom-0 mx-auto w-full max-w-[680px] px-6">
            {slash.open && <SlashMenu items={slash.items} selected={slash.selected} onPick={slash.accept} />}
            <PromptInputBar
              value={draft}
              onChange={setDraft}
              onKeyDown={onKeyDown}
              placeholder={s.taskPlaceholder}
              ariaLabel={s.taskPlaceholder}
              textareaRef={textareaRef}
              right={
                <>
                  {run.phase === 'planning' && <Loader />}
                  <Button
                    size="icon"
                    className="size-9 rounded-full"
                    aria-label={s.plan}
                    title={s.plan}
                    disabled={draft.trim() === '' || run.phase === 'planning'}
                    onClick={makeDraftPlan}
                  >
                    <SendIcon />
                  </Button>
                </>
              }
            />
          </div>
        </div>
      )}
    </div>
  );
}
