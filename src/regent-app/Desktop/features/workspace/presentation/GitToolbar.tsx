'use client';
// Commit / Commit+Push / Push. Commit is local and reversible, so it just
// runs; anything that reaches the REMOTE confirms first (shared-state action).
import { useState } from 'react';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { ConfirmDialog } from '@/shared/ui/ConfirmDialog';
import { ErrorState } from '@/shared/ui/ErrorState';
import type { GitStatus } from '@/features/workspace/domain/workspaceModel';

interface GitToolbarProps {
  readonly status?: GitStatus;
  readonly busy: boolean;
  readonly error?: string;
  readonly onClearError: () => void;
  readonly onCommit: (message: string) => Promise<boolean>;
  readonly onPush: () => Promise<boolean>;
  /** Open the full per-file diff — the change count is the affordance. */
  readonly onShowChanges: () => void;
}

export function GitToolbar({
  status,
  busy,
  error,
  onClearError,
  onCommit,
  onPush,
  onShowChanges,
}: GitToolbarProps) {
  const s = t().workspace;
  const [message, setMessage] = useState('');
  const [pending, setPending] = useState<'push' | 'commitPush'>();

  if (status === undefined) return null;
  if (!status.isRepo) {
    return <p className="px-2 py-1.5 text-[11px] text-text-tertiary">{s.notARepo}</p>;
  }

  const canCommit = status.dirty && message.trim() !== '' && !busy;
  // Push needs an upstream: without one git fails, and we deliberately don't
  // invent a remote/branch pairing on the user's behalf.
  const canPush = status.upstream !== undefined && !busy;

  const runCommit = async () => {
    if (await onCommit(message.trim())) setMessage('');
  };

  return (
    <div className="border-t border-stroke-tertiary px-2 py-2">
      <div className="mb-1.5 flex items-center gap-1.5 text-[11px] text-text-tertiary">
        <span className="truncate">{status.branch ?? s.detached}</span>
        {status.ahead > 0 && <span>↑{status.ahead}</span>}
        {status.behind > 0 && <span>↓{status.behind}</span>}
        {status.dirty && (
          <button
            type="button"
            className="cursor-pointer text-accent underline-offset-2 hover:underline"
            title={s.changesTitle}
            onClick={onShowChanges}
          >
            {s.changes(status.entries.length)}
          </button>
        )}
      </div>

      <input
        value={message}
        placeholder={s.commitPlaceholder}
        disabled={busy}
        onChange={(e) => setMessage(e.target.value)}
        className="mb-1.5 w-full rounded-sm bg-hover px-2 py-1 text-[12px] text-text-primary outline-none placeholder:text-text-tertiary"
      />

      <div className="flex flex-wrap gap-1.5">
        <Button size="sm" disabled={!canCommit} onClick={runCommit}>
          {s.commit}
        </Button>
        <Button
          size="sm"
          variant="ghost"
          disabled={!canCommit || !canPush}
          onClick={() => setPending('commitPush')}
        >
          {s.commitAndPush}
        </Button>
        <Button size="sm" variant="ghost" disabled={!canPush} onClick={() => setPending('push')}>
          {s.push}
        </Button>
      </div>

      {error !== undefined && (
        <div className="mt-1.5">
          <ErrorState compact description={error} />
          <button type="button" className="mt-1 text-[11px] text-text-tertiary" onClick={onClearError}>
            {s.dismiss}
          </button>
        </div>
      )}

      {pending !== undefined && (
        <ConfirmDialog
          title={pending === 'push' ? s.pushConfirmTitle : s.commitPushConfirmTitle}
          description={s.pushConfirmBody(status.upstream ?? '')}
          confirmLabel={s.push}
          cancelLabel={s.cancel}
          onCancel={() => setPending(undefined)}
          onConfirm={async () => {
            const step = pending;
            setPending(undefined);
            // Commit+Push composes the two calls client-side rather than
            // adding a combined RPC; a failed commit stops before pushing.
            if (step === 'commitPush' && !(await onCommit(message.trim()))) return;
            if (step === 'commitPush') setMessage('');
            await onPush();
          }}
        />
      )}
    </div>
  );
}
