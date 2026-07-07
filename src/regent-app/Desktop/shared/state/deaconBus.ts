'use client';
// THE single deacon notification subscription. One unfiltered onDeaconEvent
// listener starts lazily on first use inside the shell and fans every event
// out to (a) imperative subscribers filtered by method/session — the seam that
// replaces per-viewmodel onDeaconEvent calls — and (b) store slices any surface
// can read: per-session turn activity (idle → running once deltas stream → done
// on turn end) and the last global error. The listener is a process-lifetime
// singleton, so it is never torn down; individual subscribers unsubscribe.
import { useEffect } from 'react';
import { type DeaconEvent, onDeaconEvent } from '@/shared/infrastructure/rpc/client';
import { type Store, createStore, useStore } from '@/shared/state/store';

export type { DeaconEvent };

export type TurnActivity = 'idle' | 'running' | 'done';

interface BusState {
  readonly turns: Readonly<Record<string, TurnActivity>>;
  readonly lastError?: string;
}

export interface DeaconFilter {
  readonly method?: string;
  readonly sessionId?: string;
}

type Handler = (event: DeaconEvent) => void;
interface Sub extends DeaconFilter {
  readonly handler: Handler;
}

const store: Store<BusState> = createStore<BusState>({ turns: {} });
const subs = new Set<Sub>();
let unlisten: (() => void) | undefined;
let starting: Promise<void> | undefined;

function setTurn(sessionId: string, activity: TurnActivity): void {
  const turns = store.getState().turns;
  if (turns[sessionId] === activity) return;
  store.setState({ turns: { ...turns, [sessionId]: activity } });
}

function updateSlices(event: DeaconEvent, sessionId?: string): void {
  switch (event.method) {
    case 'message.delta':
      if (sessionId !== undefined) setTurn(sessionId, 'running');
      break;
    case 'turn.complete':
    case 'turn.interrupted': {
      if (sessionId !== undefined) setTurn(sessionId, 'done');
      const error = event.params.error;
      if (typeof error === 'string' && error !== '') store.setState({ lastError: error });
      break;
    }
    case 'deacon.exited':
      store.setState({ lastError: 'The agent backend exited.' });
      break;
    default:
      break;
  }
}

function dispatch(event: DeaconEvent): void {
  // Matches onDeaconEvent's filter: global notices (no session_id) always pass;
  // session-scoped events reach only subscribers for that session.
  const sessionId = event.params.session_id;
  updateSlices(event, sessionId);
  for (const sub of subs) {
    if (sub.method !== undefined && sub.method !== event.method) continue;
    if (sub.sessionId !== undefined && sessionId !== undefined && sessionId !== sub.sessionId) continue;
    sub.handler(event);
  }
}

function ensureStarted(): void {
  if (unlisten !== undefined || starting !== undefined) return;
  starting = onDeaconEvent(dispatch).then((fn) => {
    unlisten = fn;
  });
}

/** Imperative subscription. Returns an unsubscribe fn. Filter by `method`
 * and/or `sessionId`; omit both to receive every event. */
export function subscribe(filter: DeaconFilter, handler: Handler): () => void {
  ensureStarted();
  const sub: Sub = { ...filter, handler };
  subs.add(sub);
  return () => {
    subs.delete(sub);
  };
}

/** Turn activity for one session: 'idle' until deltas stream, 'done' at turn
 * end. Starts the bus on mount. */
export function useTurnActivity(sessionId: string | undefined): TurnActivity {
  useEffect(() => {
    ensureStarted();
  }, []);
  return useStore(store, (s) => (sessionId !== undefined ? s.turns[sessionId] ?? 'idle' : 'idle'));
}

/** The last global error seen on any turn (or a backend exit). */
export function useLastDeaconError(): string | undefined {
  useEffect(() => {
    ensureStarted();
  }, []);
  return useStore(store, (s) => s.lastError);
}
