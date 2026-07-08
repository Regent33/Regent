'use client';
// The Code surface — regent-code's plan → approve → run → verify/revert flow.
// The run log is the shared Transcript (deltas, tool rows, approval cards).
import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent } from 'react';
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
import { SlashMenu } from '@/features/chat/presentation/composer/SlashMenu';

const MAX_TEXTAREA_ROWS = 7; // grow with content, then scroll inside.

function resizeTextarea(el: HTMLTextAreaElement) {
  const style = window.getComputedStyle(el);
  const lineHeight = Number.parseFloat(style.lineHeight);
  const paddingY = Number.parseFloat(style.paddingTop) + Number.parseFloat(style.paddingBottom);
  const borderY = Number.parseFloat(style.borderTopWidth) + Number.parseFloat(style.borderBottomWidth);
  const maxHeight = lineHeight * MAX_TEXTAREA_ROWS + paddingY + borderY;

  el.style.height = 'auto';
  el.style.height = `${Math.min(el.scrollHeight, maxHeight)}px`;
  el.style.overflowY = el.scrollHeight > maxHeight ? 'auto' : 'hidden';
}

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

  useLayoutEffect(() => {
    if (idle && textareaRef.current) resizeTextarea(textareaRef.current);
  }, [draft, idle]);

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
      run.makePlan(draft);
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
          <div className="absolute inset-x-0 bottom-0 mx-auto w-full max-w-140">
            {slash.open && <SlashMenu items={slash.items} selected={slash.selected} onPick={slash.accept} />}
            <div
              className="flex items-end gap-1.5 rounded-2xl bg-bg py-1.5 pl-3 pr-1.5"
              style={{ boxShadow: 'var(--shadow-elev)' }}
            >
              <textarea
                ref={textareaRef}
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                onKeyDown={onKeyDown}
                placeholder={s.taskPlaceholder}
                rows={1}
                aria-label={s.taskPlaceholder}
                className="min-w-0 flex-1 resize-none overflow-y-hidden bg-transparent py-2 text-sm text-text-primary outline-none placeholder:text-text-tertiary"
              />
              {run.phase === 'planning' && <Loader />}
              <Button
                size="icon"
                className="size-9 rounded-full"
                aria-label={s.plan}
                title={s.plan}
                disabled={draft.trim() === '' || run.phase === 'planning'}
                onClick={() => run.makePlan(draft)}
              >
                <SendIcon />
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
