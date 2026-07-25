import type { DeaconEvent } from '@/shared/infrastructure/rpc/client';
import { type Store, createStore, useStore } from '@/shared/state/store';

export type TurnActivity = 'idle' | 'running' | 'done';

interface TurnState {
  readonly turns: Readonly<Record<string, TurnActivity>>;
  readonly errors: Readonly<Record<string, string>>;
}

const store: Store<TurnState> = createStore<TurnState>({ turns: {}, errors: {} });

export interface TurnUpdate {
  readonly activity: TurnActivity;
  /** null clears a prior error when a new turn starts or succeeds. */
  readonly error: string | null;
}

/** Persistent turn-state projection from the global deacon event stream. */
export function turnUpdate(event: DeaconEvent): TurnUpdate | undefined {
  switch (event.method) {
    case 'turn.started':
    case 'tool.start':
    case 'message.delta':
      return { activity: 'running', error: null };
    case 'turn.complete':
    case 'turn.interrupted': {
      const error = event.params.error;
      return {
        activity: 'done',
        error: typeof error === 'string' && error !== '' ? error : null,
      };
    }
    default:
      return undefined;
  }
}

/** Apply one session-scoped event to the process-lifetime turn slice. */
export function applyTurnEvent(event: DeaconEvent, sessionId: string | undefined): TurnUpdate | undefined {
  const update = turnUpdate(event);
  if (sessionId === undefined || update === undefined) return update;

  const state = store.getState();
  const activityChanged = state.turns[sessionId] !== update.activity;
  const errorChanged =
    update.error === null
      ? state.errors[sessionId] !== undefined
      : state.errors[sessionId] !== update.error;
  // message.delta is high-volume; once running, identical deltas must not wake
  // every useSyncExternalStore subscriber for every token.
  if (!activityChanged && !errorChanged) return update;

  const turns = activityChanged
    ? { ...state.turns, [sessionId]: update.activity }
    : state.turns;
  if (update.error !== null) {
    store.setState({ turns, errors: { ...state.errors, [sessionId]: update.error } });
  } else {
    const { [sessionId]: _, ...errors } = state.errors;
    store.setState({ turns, errors });
  }
  return update;
}

export const useStoredTurnActivity = (sessionId: string | undefined): TurnActivity =>
  useStore(store, (state) =>
    sessionId === undefined ? 'idle' : (state.turns[sessionId] ?? 'idle'),
  );

export const useStoredTurnError = (sessionId: string | undefined): string | undefined =>
  useStore(store, (state) =>
    sessionId === undefined ? undefined : state.errors[sessionId],
  );
