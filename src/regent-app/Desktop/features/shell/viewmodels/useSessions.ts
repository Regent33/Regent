'use client';
// Rail session list — one fetch of `session.list` (singular namespace). The
// deacon returns {session_id, source, model, message_count, started_at} rows
// (dispatcher/session_ops.rs); presentation formats them.
import { useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

export interface SessionRow {
  readonly id: string;
  readonly source?: string;
  readonly model?: string;
  readonly messageCount?: number;
}

export interface SessionsState {
  readonly sessions: readonly SessionRow[];
  readonly loading: boolean;
  readonly error?: string;
}

function toRow(value: unknown): SessionRow | undefined {
  if (typeof value !== 'object' || value === null) return undefined;
  const v = value as Record<string, unknown>;
  const id = typeof v.session_id === 'string' ? v.session_id : typeof v.id === 'string' ? v.id : undefined;
  if (id === undefined) return undefined;
  return {
    id,
    source: typeof v.source === 'string' ? v.source : undefined,
    model: typeof v.model === 'string' ? v.model : undefined,
    messageCount: typeof v.message_count === 'number' ? v.message_count : undefined,
  };
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
      const list = Array.isArray(result.value) ? result.value : [];
      setState({ sessions: list.map(toRow).filter((r): r is SessionRow => r !== undefined), loading: false });
    });
    return () => {
      alive = false;
    };
  }, []);

  return state;
}
