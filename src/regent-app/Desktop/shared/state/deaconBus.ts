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

/** Token usage for the most recent completed turn, when the backend sends the
 * (additive, may be absent) `input_tokens`/`output_tokens`/`context_max`
 * fields on `turn.complete`. Global, not per-session — the status bar shows
 * one meter for whichever turn last reported it. */
export interface UsageSnapshot {
  readonly inputTokens: number;
  readonly outputTokens: number;
  readonly contextMax: number;
}

interface BusState {
  readonly turns: Readonly<Record<string, TurnActivity>>;
  readonly lastError?: string;
  /** True once the deacon process has exited — set by the `deacon.exited`
   * notification the Rust bridge synthesizes when its stdout pipe closes. */
  readonly dead: boolean;
  readonly usage?: UsageSnapshot;
  /** The active model, from `model.changed` — fired on model.set AND when
   * applying a new primary on the Model page re-points the active model. */
  readonly model?: string;
  /** The model actually answering while the provider chain is failed over
   * (`model.failover` with engaged=true) — undefined when the primary serves.
   * Transient: cleared on recovery and on any `model.changed`. */
  readonly fallbackModel?: string;
}

export interface DeaconFilter {
  readonly method?: string;
  readonly sessionId?: string;
}

type Handler = (event: DeaconEvent) => void;
interface Sub extends DeaconFilter {
  readonly handler: Handler;
}

const store: Store<BusState> = createStore<BusState>({ turns: {}, dead: false });

/** Reads `turn.complete`'s optional usage fields — undefined unless all three
 * are present numbers, so a partial/older payload never produces a bogus
 * meter. */
function readUsage(params: Record<string, unknown>): UsageSnapshot | undefined {
  const { input_tokens: input, output_tokens: output, context_max: max } = params;
  if (typeof input !== 'number' || typeof output !== 'number' || typeof max !== 'number') return undefined;
  return { inputTokens: input, outputTokens: output, contextMax: max };
}
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
      const usage = readUsage(event.params);
      if (usage !== undefined) store.setState({ usage });
      break;
    }
    case 'deacon.exited':
      store.setState({ lastError: 'The agent backend exited.', dead: true });
      break;
    case 'model.changed': {
      const model = event.params.model;
      // A deliberate model switch resets any stale failover indicator too.
      if (typeof model === 'string' && model !== '') store.setState({ model, fallbackModel: undefined });
      break;
    }
    case 'model.failover': {
      const { engaged, model } = event.params;
      store.setState({
        fallbackModel: engaged === true && typeof model === 'string' ? model : undefined,
      });
      break;
    }
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

/** True once `deacon.exited` has fired — the backend process died mid-run.
 * Combine with a boot probe for the "never started" case (see
 * useBootHealth), which the bus alone cannot detect. */
export function useDeaconExited(): boolean {
  useEffect(() => {
    ensureStarted();
  }, []);
  return useStore(store, (s) => s.dead);
}

/** The raw token-usage snapshot backing useContextPercent — for surfaces
 * (the context status-bar popover) that want the input/output/max numbers
 * themselves rather than the derived percent. Same "undefined until a turn
 * reports it" contract. */
export function useUsageSnapshot(): UsageSnapshot | undefined {
  useEffect(() => {
    ensureStarted();
  }, []);
  return useStore(store, (s) => s.usage);
}

/** The active model per the deacon's `model.changed` events — undefined until
 * the first change this session; callers fall back to their `model.get`
 * probe for the initial value. */
export function useActiveModel(): string | undefined {
  useEffect(() => {
    ensureStarted();
  }, []);
  return useStore(store, (s) => s.model);
}

/** The model answering during a provider failover (`model.failover`), or
 * undefined while the primary serves. Cleared on recovery / model switch. */
export function useFallbackModel(): string | undefined {
  useEffect(() => {
    ensureStarted();
  }, []);
  return useStore(store, (s) => s.fallbackModel);
}

/** Context-window usage as a whole-number percent, once a turn has reported
 * it. `undefined` until the first `turn.complete` carrying the usage fields
 * arrives — callers show "—" for that gap, never a guess. */
export function useContextPercent(): number | undefined {
  useEffect(() => {
    ensureStarted();
  }, []);
  return useStore(store, (s) => {
    if (s.usage === undefined || s.usage.contextMax <= 0) return undefined;
    const used = s.usage.inputTokens + s.usage.outputTokens;
    return Math.round((used / s.usage.contextMax) * 100);
  });
}
