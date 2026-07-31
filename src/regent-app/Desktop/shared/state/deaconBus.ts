'use client';
// THE single deacon notification subscription. One unfiltered onDeaconEvent
// listener starts lazily on first use inside the shell and fans every event
// out to (a) imperative subscribers filtered by method/session — the seam that
// replaces per-viewmodel onDeaconEvent calls — and (b) store slices any surface
// can read: per-session turn activity/errors and the last global error. The
// listener is a process-lifetime singleton, so it is never torn down;
// individual subscribers unsubscribe.
import { useEffect } from 'react';
import { type DeaconEvent, onDeaconEvent } from '@/shared/infrastructure/rpc/client';
import { type Store, createStore, useStore } from '@/shared/state/store';
import {
  type TurnActivity,
  applyTurnEvent,
  useStoredTurnActivity,
  useStoredTurnError,
} from '@/shared/state/turnActivity';

export type { DeaconEvent };
export type { TurnActivity } from '@/shared/state/turnActivity';

/** What the most recent completed turn SPENT: prompt + completion tokens summed
 * across every model call in that turn, from `turn.complete`. An agentic turn
 * re-sends the prompt on each call, so this routinely exceeds `contextMax` many
 * times over — it is a cost signal, never a fill signal. See
 * [ContextSnapshot] for how full the window actually is. */
export interface UsageSnapshot {
  readonly inputTokens: number;
  readonly outputTokens: number;
  readonly contextMax: number;
}

/** How FULL the context window is after the last turn, from `turn.usage`:
 * the prompt the next call would carry (history + system prompt + tool
 * schemas) against the active model's window. This is what the ctx meter
 * shows. Global, not per-session — whichever turn last reported. */
export interface ContextSnapshot {
  readonly contextTokens: number;
  readonly maxContextTokens: number;
  /** Slice of `contextTokens` that is tool schemas — fixed per turn and not
   * reducible by compaction. */
  readonly toolSchemaTokens: number;
  /** Estimate at which compaction summarizes history and splits the session.
   * `undefined` when compaction cannot fire (disabled, or breaker open). */
  readonly compactAtTokens?: number;
}

interface BusState {
  readonly lastError?: string;
  /** True once the deacon process has exited — set by the `deacon.exited`
   * notification the Rust bridge synthesizes when its stdout pipe closes. */
  readonly dead: boolean;
  readonly usage?: UsageSnapshot;
  readonly context?: ContextSnapshot;
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

const store: Store<BusState> = createStore<BusState>({ dead: false });

/** Reads `turn.complete`'s optional usage fields — undefined unless all three
 * are present numbers, so a partial/older payload never produces a bogus
 * meter. */
function readUsage(params: Record<string, unknown>): UsageSnapshot | undefined {
  const { input_tokens: input, output_tokens: output, context_max: max } = params;
  if (typeof input !== 'number' || typeof output !== 'number' || typeof max !== 'number') return undefined;
  return { inputTokens: input, outputTokens: output, contextMax: max };
}

/** Reads `turn.usage`'s context-fill fields. Same all-or-nothing contract as
 * [readUsage]: a partial payload yields undefined rather than a bogus meter.
 * `compact_at_tokens` is optional on its own — it is legitimately null when
 * compaction can't fire, which is not a reason to drop the whole snapshot. */
export function readContext(params: Record<string, unknown>): ContextSnapshot | undefined {
  const {
    context_tokens: used,
    max_context_tokens: max,
    tool_schema_tokens: schemas,
    compact_at_tokens: compactAt,
  } = params;
  if (typeof used !== 'number' || typeof max !== 'number' || max <= 0) return undefined;
  return {
    contextTokens: used,
    maxContextTokens: max,
    toolSchemaTokens: typeof schemas === 'number' ? schemas : 0,
    compactAtTokens: typeof compactAt === 'number' && compactAt > 0 ? compactAt : undefined,
  };
}
const subs = new Set<Sub>();
let unlisten: (() => void) | undefined;
let starting: Promise<void> | undefined;

function updateSlices(event: DeaconEvent, sessionId?: string): void {
  const turn = applyTurnEvent(event, sessionId);

  switch (event.method) {
    case 'turn.complete':
    case 'turn.interrupted': {
      if (turn?.error !== null && turn?.error !== undefined) {
        store.setState({ lastError: turn.error });
      }
      const usage = readUsage(event.params);
      if (usage !== undefined) store.setState({ usage });
      break;
    }
    case 'turn.usage': {
      const context = readContext(event.params);
      if (context !== undefined) store.setState({ context });
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

/** Turn activity for one session: running from turn.started/tool work through
 * completion. Starts the bus on mount. */
export function useTurnActivity(sessionId: string | undefined): TurnActivity {
  useEffect(() => {
    ensureStarted();
  }, []);
  return useStoredTurnActivity(sessionId);
}

/** Error from the latest completed turn for this session, cleared on its next
 * turn start. This survives route unmounts even though chat-local state does not. */
export function useTurnError(sessionId: string | undefined): string | undefined {
  useEffect(() => {
    ensureStarted();
  }, []);
  return useStoredTurnError(sessionId);
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

/** The raw context-fill snapshot — for the popover, which shows the token
 * numbers and the compaction landmark rather than the derived percent. */
export function useContextSnapshot(): ContextSnapshot | undefined {
  useEffect(() => {
    ensureStarted();
  }, []);
  return useStore(store, (s) => s.context);
}

/** How full the context window is, as a whole-number percent, once a turn has
 * reported it. `undefined` until the first `turn.usage` arrives — callers show
 * "—" for that gap, never a guess.
 *
 * Reads context FILL, not turn spend. The two are unrelated quantities: a turn
 * that makes 40 tool calls spends ~40x the prompt but leaves the window barely
 * fuller than one that makes none, so dividing spend by the window printed
 * "388%" on a half-empty context (owner repro 2026-07-31). */
export function useContextPercent(): number | undefined {
  useEffect(() => {
    ensureStarted();
  }, []);
  return useStore(store, (s) => contextPercentOf(s.context));
}

/** Fill percent from a snapshot — pure, so the "fill not spend" invariant is
 * testable without a renderer. */
export function contextPercentOf(context: ContextSnapshot | undefined): number | undefined {
  if (context === undefined) return undefined;
  return Math.round((context.contextTokens / context.maxContextTokens) * 100);
}

/** Whether fill has reached the compaction threshold — pure, see
 * [contextPercentOf]. False when the backend reports no threshold. */
export function compactionImminentOf(context: ContextSnapshot | undefined): boolean {
  if (context === undefined || context.compactAtTokens === undefined) return false;
  return context.contextTokens >= context.compactAtTokens;
}

/** True once context fill has crossed the compaction threshold — the next turn
 * summarizes history and continues in a child session. The one context event a
 * user should see coming; `false` when the backend reports no threshold. */
export function useCompactionImminent(): boolean {
  useEffect(() => {
    ensureStarted();
  }, []);
  return useStore(store, (s) => compactionImminentOf(s.context));
}
