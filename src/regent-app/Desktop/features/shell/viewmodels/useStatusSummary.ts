'use client';
// Status-bar cron/agents counts — `cron.list` (enabled/total), `agents.list`
// (count) and `status.get` (active sessions + the earliest enabled cron's
// next run), refreshed on mount and every 60s. Interval instead of
// open-triggered fetch: least code, and all three calls are small enough
// that polling isn't wasteful.
import { useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

const REFRESH_MS = 60_000;

export interface StatusSummary {
  readonly cronEnabled?: number;
  readonly cronTotal?: number;
  readonly cronNextRunAt?: number;
  readonly agentsCount?: number;
  readonly activeSessions?: number;
}

export function useStatusSummary(): StatusSummary {
  const [summary, setSummary] = useState<StatusSummary>({});

  useEffect(() => {
    if (!isTauri()) return;
    let alive = true;

    const refresh = () => {
      void deaconRequest('cron.list', {}).then((result) => {
        if (!alive || !result.ok) return;
        const list = Array.isArray(result.value) ? result.value : [];
        const enabled = list.filter(
          (j) => typeof j === 'object' && j !== null && (j as Record<string, unknown>).enabled === true,
        ).length;
        setSummary((prev) => ({ ...prev, cronEnabled: enabled, cronTotal: list.length }));
      });
      void deaconRequest('agents.list', {}).then((result) => {
        if (!alive || !result.ok) return;
        const list = Array.isArray(result.value) ? result.value : [];
        setSummary((prev) => ({ ...prev, agentsCount: list.length }));
      });
      void deaconRequest('status.get', {}).then((result) => {
        if (!alive || !result.ok) return;
        const v = (result.value ?? {}) as Record<string, unknown>;
        const activeSessions = typeof v.active_sessions === 'number' ? v.active_sessions : undefined;
        const cron = typeof v.cron === 'object' && v.cron !== null ? (v.cron as Record<string, unknown>) : undefined;
        const cronNextRunAt = typeof cron?.next_run_at === 'number' ? cron.next_run_at : undefined;
        setSummary((prev) => ({ ...prev, activeSessions, cronNextRunAt }));
      });
    };

    refresh();
    const timer = setInterval(refresh, REFRESH_MS);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);

  return summary;
}
