'use client';
// Session list + mutations over `session.list` (singular namespace), held in
// ONE module-level store shared by every consumer (rail, titlebar menu,
// messaging, settings/Archived). One fetch serves them all, and an optimistic
// mutation from any surface reflects everywhere instantly — per-component
// copies used to fetch 1000 rows each and drift apart. Mutations
// (rename/pin/archive/delete) optimistic-update the store, then refetch on a
// failed response — same pattern as useCronJobs.
import { useEffect } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';
import { subscribe } from '@/shared/state/deaconBus';
import { createStore, useStore } from '@/shared/state/store';

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

interface SessionsSlice {
  readonly sessions: readonly SessionRow[];
  readonly loading: boolean;
  readonly error?: string;
}

// Initial state must be environment-independent: the static prerender and the
// first client render inside Tauri have to produce identical HTML (hydration).
// The shell check happens on first use, never at module init.
const store = createStore<SessionsSlice>({ sessions: [], loading: true });
let started = false;
let backfillDone = false;

async function fetchList(): Promise<void> {
  // ponytail: deacon defaults limit to 20 and internal sources are filtered
  // AFTER the limit — a burst of curator runs would empty the rail. 1000
  // covers the store today (~950); paginate if the store outgrows it.
  const result = await deaconRequest('session.list', { limit: 1000 });
  if (!result.ok) {
    store.setState({ error: result.error.message, loading: false });
    return;
  }
  const list = Array.isArray(result.value) ? result.value : [];
  store.setState({
    sessions: list
      .map(toRow)
      .filter((r): r is SessionRow => r !== undefined && !INTERNAL_SOURCES.has(r.source ?? '')),
    error: undefined,
    loading: false,
  });
}

/** Refetch the shared list — for surfaces whose sessions are created outside
 * the desktop's deacon events (Butler's voice calls land in the same store on
 * disk but stream no notification here). No-op outside Tauri. */
export function refreshSessions(): void {
  if (isTauri()) void fetchList();
}

/** One-shot per app run: name pre-titling sessions so the rail stops showing
 * "deacon · 3f9c2a". The deacon replies `{started}` immediately and sweeps
 * DETACHED (its dispatcher is serial — an awaited sweep once froze every
 * other request behind ~30 model calls); each landed title streams in as a
 * `session.titled` event the subscription below already patches in. */
async function backfillTitlesOnce(): Promise<void> {
  if (backfillDone) return;
  backfillDone = true;
  const r = await deaconRequest('session.backfill_titles', { limit: 30 });
  if (!r.ok) return;
  // Back-compat: a pre-detach deacon replies with the finished counts instead.
  const v = r.value as { titled?: number };
  if (typeof v.titled === 'number' && v.titled > 0) void fetchList();
}

function ensureStarted(): void {
  if (started) return;
  started = true;
  if (!isTauri()) {
    store.setState({ loading: false });
    return;
  }
  void fetchList().then(backfillTitlesOnce);

  // A brand-new chat creates its session lazily on first submit — when a turn
  // starts for a session the list doesn't know yet, refetch so it appears live.
  subscribe({ method: 'turn.started' }, (event) => {
    const id = event.params.session_id;
    if (typeof id === 'string' && !store.getState().sessions.some((s) => s.id === id)) {
      void fetchList();
    }
  });

  // First-turn titling announces `session.titled` — patch the matching row's
  // title in place rather than refetching. Older binaries never send it.
  subscribe({ method: 'session.titled' }, (event) => {
    const id = event.params.session_id;
    const title = event.params.title;
    if (typeof id !== 'string' || typeof title !== 'string') return;
    store.setState((prev) => ({ sessions: prev.sessions.map((s) => (s.id === id ? { ...s, title } : s)) }));
  });
}

function patch(id: string, change: Partial<SessionRow>): void {
  store.setState((prev) => ({ sessions: prev.sessions.map((s) => (s.id === id ? { ...s, ...change } : s)) }));
}

function requestThenSync(method: string, params: Record<string, unknown>): void {
  void deaconRequest(method, params).then((result) => {
    if (!result.ok) {
      store.setState({ error: result.error.message });
      void fetchList(); // roll the optimistic update back to disk truth
    }
  });
}

function rename(id: string, title: string): void {
  patch(id, { title });
  requestThenSync('session.rename', { session_id: id, title });
}

function togglePin(id: string): void {
  const next = !store.getState().sessions.find((s) => s.id === id)?.pinned;
  patch(id, { pinned: next });
  requestThenSync('session.pin', { session_id: id, pinned: next });
}

function toggleArchive(id: string): void {
  const next = !store.getState().sessions.find((s) => s.id === id)?.archived;
  patch(id, { archived: next });
  requestThenSync('session.archive', { session_id: id, archived: next });
}

function remove(id: string): void {
  store.setState((prev) => ({ sessions: prev.sessions.filter((s) => s.id !== id) }));
  requestThenSync('session.delete', { session_id: id });
}

export function useSessions(): SessionsState {
  useEffect(() => {
    ensureStarted();
  }, []);
  const sessions = useStore(store, (s) => s.sessions);
  const loading = useStore(store, (s) => s.loading);
  const error = useStore(store, (s) => s.error);
  return { sessions, loading, error, rename, togglePin, toggleArchive, remove };
}
