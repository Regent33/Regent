'use client';
// One chat session: lazy `session.create` on first submit (or `session.resume`
// + `session.history` seeding for an existing id — fired in parallel, with a
// `resuming` flag so the UI shows progress), wire events mapped into the pure
// transcript reducer. Events are the source of truth for turn state — the
// `prompt.submit` response only resolves when the whole turn ends, and its
// -32000 failures duplicate the `turn.complete {error}` notification, so
// those are ignored here. Mutating tool actions arrive as `approval.request`
// and MUST be answered (`approval.respond`) or the deacon denies at 120s.
import { useCallback, useEffect, useReducer, useRef, useState } from 'react';
import {
  type DeaconEvent,
  deaconRequest,
  isTauri,
  onDeaconEvent,
} from '@/shared/infrastructure/rpc/client';
import {
  type TranscriptItem,
  type TranscriptState,
  emptyTranscript,
  reduceTranscript,
} from '@/features/chat/domain/transcript';

export interface ChatSession {
  readonly state: TranscriptState;
  readonly resuming: boolean;
  readonly submit: (text: string) => void;
  readonly stop: () => void;
  readonly respondApproval: (approved: boolean) => void;
}

const RPC_TURN_ERROR = -32000; // already delivered via turn.complete {error}

interface HistoryRow {
  readonly role?: string;
  readonly text?: string;
  readonly reasoning?: string | null;
  readonly tool_calls?: readonly string[];
}

/** One stored row → transcript items (thinking → text → tool rows). */
function rowToItems(m: HistoryRow): TranscriptItem[] {
  const items: TranscriptItem[] = [];
  if (m.role !== 'user' && m.role !== 'assistant') return items;
  if (typeof m.reasoning === 'string' && m.reasoning !== '') {
    items.push({ kind: 'thinking', text: m.reasoning });
  }
  if (typeof m.text === 'string' && m.text !== '') {
    items.push(
      m.role === 'user'
        ? { kind: 'user', text: m.text }
        : { kind: 'assistant', text: m.text, streaming: false },
    );
  }
  for (const name of m.tool_calls ?? []) {
    if (typeof name === 'string') items.push({ kind: 'tool', name, done: true });
  }
  return items;
}

export function useChatSession(initialSessionId?: string): ChatSession {
  const [state, dispatch] = useReducer(reduceTranscript, emptyTranscript);
  const [resuming, setResuming] = useState(false);
  const sessionRef = useRef<string | undefined>(undefined);
  const unlistenRef = useRef<(() => void) | undefined>(undefined);
  const aliveRef = useRef(true);

  const onEvent = useCallback((event: DeaconEvent) => {
    if (!aliveRef.current) return;
    const p = event.params;
    switch (event.method) {
      case 'message.delta':
        if (typeof p.text === 'string') dispatch({ type: 'delta', text: p.text });
        break;
      case 'message.complete':
        if (typeof p.reply === 'string') dispatch({ type: 'reply', text: p.reply });
        break;
      case 'tool.start':
        if (typeof p.tool === 'string') dispatch({ type: 'tool-start', name: p.tool });
        break;
      case 'tool.complete':
        if (typeof p.tool === 'string') {
          dispatch({ type: 'tool-end', name: p.tool, isError: p.is_error === true });
        }
        break;
      case 'approval.request':
        dispatch({
          type: 'approval',
          tool: typeof p.tool === 'string' ? p.tool : '?',
          action: typeof p.action === 'string' ? p.action : '',
          reason: typeof p.reason === 'string' ? p.reason : '',
        });
        break;
      case 'turn.complete':
      case 'turn.interrupted':
        dispatch({
          type: 'ended',
          error: typeof p.error === 'string' ? p.error : undefined,
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

  // Resume an existing session on mount and seed its stored transcript; a new
  // session is created lazily on the first submit instead.
  useEffect(() => {
    aliveRef.current = true;
    if (initialSessionId !== undefined && isTauri()) {
      setResuming(true);
      void (async () => {
        // Independent calls — history is a pure read; run them concurrently.
        const [resumed, history] = await Promise.all([
          deaconRequest('session.resume', { session_id: initialSessionId }),
          deaconRequest<HistoryRow[]>('session.history', { session_id: initialSessionId }),
        ]);
        if (!aliveRef.current) return;
        setResuming(false);
        if (!resumed.ok) {
          dispatch({ type: 'failed', message: resumed.error.message });
          return;
        }
        await attach(initialSessionId);
        if (history.ok && Array.isArray(history.value)) {
          const items = history.value.flatMap(rowToItems);
          if (items.length > 0) dispatch({ type: 'seeded', items });
        }
      })();
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

  const respondApproval = useCallback((approved: boolean) => {
    const sessionId = sessionRef.current;
    if (sessionId === undefined) return;
    dispatch({ type: 'approval-resolved', approved });
    void deaconRequest('approval.respond', { session_id: sessionId, approved });
  }, []);

  return { state, resuming, submit, stop, respondApproval };
}
