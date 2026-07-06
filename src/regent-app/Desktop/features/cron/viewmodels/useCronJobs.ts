'use client';
// Cron jobs — cron.list (admin_ops.rs::cron_list) returns
// {id, name, prompt, enabled, next_run_at, last_run_at} (epoch seconds, or
// null for last_run_at until a job has fired). No raw cron expression comes
// back from the list handler, so the schedule column shows next/last run
// instead (a deliberate deviation — see the task report).
import { useCallback, useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

export interface CronJobRow {
  readonly id: string;
  readonly name: string;
  readonly prompt?: string;
  readonly enabled: boolean;
  readonly nextRunAt?: number;
  readonly lastRunAt?: number;
}

export interface CronJobsState {
  readonly jobs: readonly CronJobRow[];
  readonly loading: boolean;
  readonly error?: string;
  readonly setEnabled: (id: string, enabled: boolean) => void;
  readonly runNow: (id: string) => void;
  readonly remove: (id: string) => void;
}

function toRow(value: unknown): CronJobRow | undefined {
  if (typeof value !== 'object' || value === null) return undefined;
  const v = value as Record<string, unknown>;
  const id = typeof v.id === 'string' ? v.id : undefined;
  const name = typeof v.name === 'string' ? v.name : undefined;
  if (id === undefined || name === undefined) return undefined;
  return {
    id,
    name,
    prompt: typeof v.prompt === 'string' ? v.prompt : undefined,
    enabled: v.enabled === true,
    nextRunAt: typeof v.next_run_at === 'number' ? v.next_run_at : undefined,
    lastRunAt: typeof v.last_run_at === 'number' ? v.last_run_at : undefined,
  };
}

export function useCronJobs(): CronJobsState {
  const [jobs, setJobs] = useState<readonly CronJobRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const [reload, setReload] = useState(0);

  useEffect(() => {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    let alive = true;
    setLoading(true);
    void deaconRequest('cron.list', {}).then((result) => {
      if (!alive) return;
      if (!result.ok) {
        setError(result.error.message);
        setLoading(false);
        return;
      }
      const list = Array.isArray(result.value) ? result.value : [];
      setJobs(list.map(toRow).filter((j): j is CronJobRow => j !== undefined));
      setError(undefined);
      setLoading(false);
    });
    return () => {
      alive = false;
    };
  }, [reload]);

  const refetch = useCallback(() => setReload((n) => n + 1), []);

  const setEnabled = useCallback(
    (id: string, enabled: boolean) => {
      setJobs((prev) => prev.map((j) => (j.id === id ? { ...j, enabled } : j)));
      void deaconRequest('cron.set_enabled', { id, enabled }).then((result) => {
        if (!result.ok) {
          setError(result.error.message);
          refetch();
        }
      });
    },
    [refetch],
  );

  const runNow = useCallback(
    (id: string) => {
      void deaconRequest('cron.run', { id }).then((result) => {
        if (!result.ok) {
          setError(result.error.message);
          return;
        }
        refetch();
      });
    },
    [refetch],
  );

  const remove = useCallback(
    (id: string) => {
      setJobs((prev) => prev.filter((j) => j.id !== id));
      void deaconRequest('cron.remove', { id }).then((result) => {
        if (!result.ok) {
          setError(result.error.message);
          refetch();
        }
      });
    },
    [refetch],
  );

  return { jobs, loading, error, setEnabled, runNow, remove };
}
