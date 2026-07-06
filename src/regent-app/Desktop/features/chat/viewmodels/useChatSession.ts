'use client';
// One chat session: lazy `session.create` on first submit (or `session.resume`
// for an existing id), wire events mapped into the pure transcript reducer.
// Events are the source of truth for turn state — the `prompt.submit` response
// only resolves when the whole turn ends, and its -32000 failures duplicate
// the `turn.complete {error}` notification, so those are ignored here.
// Session history isn't exposed over RPC yet, so a resumed session starts
// with an empty transcript (known ceiling; additive backend change later).
import { useCallback, useEffect, useReducer, useRef } from 'react';
import {
  type DeaconEvent,
  deaconRequest,
  isTauri,
  onDeaconEvent,
} from '@/shared/infrastructure/rpc/client';
import {
  type TranscriptState,
  emptyTranscript,
  reduceTranscript,
} from '@/features/chat/domain/transcript';

export interface ChatSession {
  readonly state: TranscriptState;
  readonly submit: (text: string) => void;
  readonly stop: () => void;
}

const RPC_TURN_ERROR = -32000; // already delivered via turn.complete {error}

export function useChatSession(initialSessionId?: string): ChatSession {
  const [state, dispatch] = useReducer(reduceTranscript, emptyTranscript);
  const sessionRef = useRef<string | undefined>(undefined);
  const unlistenRef = useRef<(() => void) | undefined>(undefined);
  const aliveRef = useRef(true);

  const onEvent = useCallback((event: DeaconEvent) => {
    if (!aliveRef.current) return;
    switch (event.method) {
      case 'message.delta':
        if (typeof event.params.text === 'string') dispatch({ type: 'delta', text: event.params.text });
        break;
      case 'message.complete':
        if (typeof event.params.reply === 'string') dispatch({ type: 'reply', text: event.params.reply });
        break;
      case 'turn.complete':
      case 'turn.interrupted':
        dispatch({
          type: 'ended',
          error: typeof event.params.error === 'string' ? event.params.error : undefined,
        });
        break;
      case 'deacon.exited':
        dispatch({ type: 'failed', message: 'The agent backend exited.' });
        break;
      default:
        break;
    }
  }, []);

  const attach = useCallback(
    async (sessionId: string) => {
      sessionRef.current = sessionId;
      unlistenRef.current?.();
      unlistenRef.current = await onDeaconEvent(onEvent, sessionId);
    },
    [onEvent],
  );

  // Resume an existing session on mount; a new session is created lazily on
  // the first submit instead (Home stays cheap until the user speaks).
  useEffect(() => {
    aliveRef.current = true;
    if (initialSessionId !== undefined && isTauri()) {
      void deaconRequest('session.resume', { session_id: initialSessionId }).then((r) => {
        if (!aliveRef.current) return;
        if (r.ok) void attach(initialSessionId);
        else dispatch({ type: 'failed', message: r.error.message });
      });
    }
    return () => {
      aliveRef.current = false;
      unlistenRef.current?.();
      unlistenRef.current = undefined;
    };
  }, [initialSessionId, attach]);

  const submit = useCallback(
    (text: string) => {
      void (async () => {
        let sessionId = sessionRef.current;
        if (sessionId === undefined) {
          const created = await deaconRequest<{ session_id?: string }>('session.create', {});
          if (!aliveRef.current) return;
          if (!created.ok || typeof created.value?.session_id !== 'string') {
            dispatch({
              type: 'failed',
              message: created.ok ? 'session.create returned no id' : created.error.message,
            });
            return;
          }
          sessionId = created.value.session_id;
          await attach(sessionId);
        }
        dispatch({ type: 'submitted', text });
        const result = await deaconRequest('prompt.submit', { session_id: sessionId, text });
        if (!aliveRef.current || result.ok) return;
        const code = (result.error.cause as { code?: number } | undefined)?.code;
        // rpc turn failures arrived as turn.complete {error}; report the rest
        // (bridge dead, boundary rejection) since no event will follow.
        if (result.error.kind !== 'rpc' || code !== RPC_TURN_ERROR) {
          dispatch({ type: 'failed', message: result.error.message });
        }
      })();
    },
    [attach],
  );

  const stop = useCallback(() => {
    const sessionId = sessionRef.current;
    if (sessionId !== undefined) void deaconRequest('turn.interrupt', { session_id: sessionId });
  }, []);

  return { state, submit, stop };
}
