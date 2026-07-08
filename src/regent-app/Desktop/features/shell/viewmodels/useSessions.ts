'use client';
// Rail session list — one fetch of `session.list` (singular namespace). The
// deacon returns {session_id, source, model, message_count, started_at,
// title, pinned, archived} rows (dispatcher/session_ops.rs); presentation
// formats and groups them. Mutations (rename/pin/archive/delete) optimistic-
// update the local list, then refetch on a failed response — same pattern as
// useCronJobs.
import { useCallback, useEffect, useRef, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';
import { subscribe } from '@/shared/state/deaconBus';

// Background/agent-internal session sources are NOT user chats — the memory
// curator ("review"), cron, and board runs pack a whole transcript into one
// message and would render as a non-conversational blob. Keep them out of the
// rail (they still exist in the store; this is a display filter).
const INTERNAL_SOURCES = new Set(['review', 'curator', 'cron', 'board']);

export interface SessionRow {
  readonly id: string;
  readonly source?: string;
  readonly model?: string;
  readonly messageCount?: number;
  readonly title?: string;
  readonly pinned: boolean;
  readonly archived: boolean;
  /** ISO-ish string or epoch — only ever compared, never parsed. */
  readonly startedAt?: string;
}

export interface SessionsState {
  readonly sessions: readonly SessionRow[];
  readonly loading: boolean;
  readonly error?: string;
  readonly rename: (id: string, title: string) => void;
  readonly togglePin: (id: string) => void;
  readonly toggleArchive: (id: string) => void;
  readonly remove: (id: string) => void;
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
    title: typeof v.title === 'string' && v.title !== '' ? v.title : undefined,
    pinned: v.pinned === true,
    archived: v.archived === true,
    startedAt:
      typeof v.started_at === 'string'
        ? v.started_at
        : typeof v.started_at === 'number'
          ? String(v.started_at)
          : undefined,
  };
}

export function useSessions(): SessionsState {
  // Initial state must be environment-independent: the static prerender and
  // the first client render inside Tauri have to produce identical HTML
  // (hydration). The shell check happens in the effect, never at init.
  const [sessions, setSessions] = useState<readonly SessionRow[]>([]);
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
    // ponytail: deacon defaults limit to 20 and internal sources are filtered
    // AFTER the limit — a burst of curator runs would empty the rail. 1000
    // covers the store today (~950); paginate if the store outgrows it.
    void deaconRequest('session.list', { limit: 1000 }).then((result) => {
      if (!alive) return;
      if (!result.ok) {
        setError(result.error.message);
        setLoading(false);
        return;
      }
      const list = Array.isArray(result.value) ? result.value : [];
      setSessions(
        list
          .map(toRow)
          .filter((r): r is SessionRow => r !== undefined && !INTERNAL_SOURCES.has(r.source ?? '')),
      );
      setError(undefined);
      setLoading(false);
    });
    return () => {
      alive = false;
    };
  }, [reload]);

  // A brand-new chat creates its session lazily on first submit — when a turn
  // starts for a session the rail doesn't know yet, refetch so it appears live.
  const idsRef = useRef<ReadonlySet<string>>(new Set());
  useEffect(() => {
    idsRef.current = new Set(sessions.map((s) => s.id));
  }, [sessions]);
  useEffect(() => {
    return subscribe({ method: 'turn.started' }, (event) => {
      const id = event.params.session_id;
      if (typeof id === 'string' && !idsRef.current.has(id)) setReload((n) => n + 1);
    });
  }, []);

  // `session.titled` may start arriving from a parallel backend batch — patch
  // the matching row's title in place rather than refetching the whole list.
  // Guarded: older binaries simply never send it.
  useEffect(() => {
    return subscribe({ method: 'session.titled' }, (event) => {
      const id = event.params.session_id;
      const title = event.params.title;
      if (typeof id !== 'string' || typeof title !== 'string') return;
      setSessions((prev) => prev.map((s) => (s.id === id ? { ...s, title } : s)));
    });
  }, []);

  const refetch = useCallback(() => setReload((n) => n + 1), []);

  const rename = useCallback(
    (id: string, title: string) => {
      setSessions((prev) => prev.map((s) => (s.id === id ? { ...s, title } : s)));
      void deaconRequest('session.rename', { session_id: id, title }).then((result) => {
        if (!result.ok) {
          setError(result.error.message);
          refetch();
        }
      });
    },
    [refetch],
  );

  const togglePin = useCallback(
    (id: string) => {
      const next = !sessions.find((s) => s.id === id)?.pinned;
      setSessions((prev) => prev.map((s) => (s.id === id ? { ...s, pinned: next } : s)));
      void deaconRequest('session.pin', { session_id: id, pinned: next }).then((result) => {
        if (!result.ok) {
          setError(result.error.message);
          refetch();
        }
      });
    },
    [sessions, refetch],
  );

  const toggleArchive = useCallback(
    (id: string) => {
      const next = !sessions.find((s) => s.id === id)?.archived;
      setSessions((prev) => prev.map((s) => (s.id === id ? { ...s, archived: next } : s)));
      void deaconRequest('session.archive', { session_id: id, archived: next }).then((result) => {
        if (!result.ok) {
          setError(result.error.message);
          refetch();
        }
      });
    },
    [sessions, refetch],
  );

  const remove = useCallback(
    (id: string) => {
      setSessions((prev) => prev.filter((s) => s.id !== id));
      void deaconRequest('session.delete', { session_id: id }).then((result) => {
        if (!result.ok) {
          setError(result.error.message);
          refetch();
        }
      });
    },
    [refetch],
  );

  return { sessions, loading, error, rename, togglePin, toggleArchive, remove };
}
