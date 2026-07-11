'use client';
// Status-bar cron/agents counts — `cron.list` (enabled/total), `agents.list`
// (count) and `status.get` (active sessions + the earliest enabled cron's
// next run), refreshed on mount, every 60s, AND at each turn end — a turn may
// have created agents/cron jobs through the in-process `regent` tool, and the
// strip should show them immediately, not up to a minute later.
import { useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';
import { subscribe } from '@/shared/state/deaconBus';

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
    const unsub = subscribe({ method: 'turn.complete' }, refresh);
    return () => {
      alive = false;
      clearInterval(timer);
      unsub();
    };
  }, []);

  return summary;
}
