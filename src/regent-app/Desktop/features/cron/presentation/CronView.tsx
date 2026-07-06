'use client';
// Cron — cron.list rows with an enabled toggle (cron.set_enabled), a
// Run now action (cron.run), and a two-click "are you sure" Remove
// (cron.remove). Empty state when there are no jobs.
import { useState } from 'react';
import { t } from '@/shared/i18n/t';
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { EmptyState } from '@/shared/ui/EmptyState';
import { Button } from '@/shared/ui/Button';
import { useCronJobs } from '@/features/cron/viewmodels/useCronJobs';

function formatEpoch(seconds: number | undefined, never: string): string {
  if (seconds === undefined) return never;
  return new Date(seconds * 1000).toLocaleString();
}

export function CronView() {
  const s = t().cron;
  const { jobs, loading, error, setEnabled, runNow, remove } = useCronJobs();
  const [confirmingId, setConfirmingId] = useState<string>();

  const onRemove = (id: string) => {
    if (confirmingId === id) {
      remove(id);
      setConfirmingId(undefined);
    } else {
      setConfirmingId(id);
    }
  };

  return (
    <div className="p-6">
      <h1 className="text-lg font-semibold text-text-primary">{s.title}</h1>

      {loading && (
        <div className="mt-4">
          <Loader />
        </div>
      )}
      {error !== undefined && <ErrorState description={error} />}
      {!loading && error === undefined && jobs.length === 0 && (
        <div className="mt-6">
          <EmptyState title={s.empty} />
        </div>
      )}

      {jobs.map((job) => (
        <div key={job.id} className="mt-3 rounded-[6px] bg-hover px-3 py-2.5">
          <div className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              <p className="truncate text-sm font-semibold text-text-primary">{job.name}</p>
              {job.prompt !== undefined && (
                <p className="mt-0.5 truncate text-xs text-text-tertiary">{job.prompt}</p>
              )}
              <p className="mt-1 text-xs text-text-tertiary">
                {s.nextRun}: {formatEpoch(job.nextRunAt, s.never)} · {s.lastRun}:{' '}
                {formatEpoch(job.lastRunAt, s.never)}
              </p>
            </div>
            <div className="flex shrink-0 gap-2">
              <Button variant="secondary" size="sm" onClick={() => setEnabled(job.id, !job.enabled)}>
                {job.enabled ? s.disable : s.enable}
              </Button>
              <Button variant="secondary" size="sm" onClick={() => runNow(job.id)}>
                {s.runNow}
              </Button>
              <Button
                variant={confirmingId === job.id ? 'default' : 'ghost'}
                size="sm"
                onClick={() => onRemove(job.id)}
              >
                {confirmingId === job.id ? s.removeConfirm : s.remove}
              </Button>
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}
