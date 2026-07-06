'use client';
// Rail session list — one fetch of `session.list` (singular namespace — the
// deacon dispatcher's actual method name) with a stale-response guard.
import { useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

export interface SessionRow {
  readonly id: string;
  readonly title: string;
}

export interface SessionsState {
  readonly sessions: readonly SessionRow[];
  readonly loading: boolean;
  readonly error?: string;
}

/** The deacon's row shape isn't pinned yet — read defensively. */
function toRow(value: unknown): SessionRow | undefined {
  if (typeof value !== 'object' || value === null) return undefined;
  const v = value as Record<string, unknown>;
  const id = typeof v.id === 'string' ? v.id : typeof v.session_id === 'string' ? v.session_id : undefined;
  if (id === undefined) return undefined;
  const title = typeof v.title === 'string' && v.title !== '' ? v.title : typeof v.name === 'string' ? v.name : id;
  return { id, title };
}

export function useSessions(): SessionsState {
  const [state, setState] = useState<SessionsState>({ sessions: [], loading: isTauri() });

  useEffect(() => {
    if (!isTauri()) return;
    let alive = true;
    void deaconRequest('session.list', {}).then((result) => {
      if (!alive) return;
      if (!result.ok) {
        setState({ sessions: [], loading: false, error: result.error.message });
        return;
      }
      const raw = result.value;
      const list = Array.isArray(raw)
        ? raw
        : typeof raw === 'object' && raw !== null && Array.isArray((raw as Record<string, unknown>).sessions)
          ? ((raw as Record<string, unknown>).sessions as unknown[])
          : [];
      setState({ sessions: list.map(toRow).filter((r): r is SessionRow => r !== undefined), loading: false });
    });
    return () => {
      alive = false;
    };
  }, []);

  return state;
}
