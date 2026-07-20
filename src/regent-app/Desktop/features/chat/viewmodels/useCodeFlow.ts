'use client';
// Follow the code flow while this chat's code_task runs: child plan/execute
// sessions announce themselves (code.planning / code.started) and their tool
// calls render into this transcript, Cursor-style; verify rounds and the
// revert backstop land as notice lines. Gated on codeActiveRef — a code task
// launched from another surface never bleeds into this transcript.
import { useCallback, useEffect, useRef } from 'react';
import { t } from '@/shared/i18n/t';
import { type DeaconEvent, subscribe } from '@/shared/state/deaconBus';
import { reduceTranscript } from '@/shared/kernel/transcript';
import { clip, codeFromArgs, detailFromArgs, detailFromResult } from '@/features/chat/data/eventDetails';

type TranscriptDispatch = React.Dispatch<Parameters<typeof reduceTranscript>[1]>;
type Ref<T> = { current: T };

// The code-flow notifications a chat follows while ITS code_task runs (one
// code task runs process-wide at a time, so ownership is unambiguous).
const CODE_METHODS = ['code.planning', 'code.started', 'code.verify', 'code.reverted'] as const;

/**
 * Wires the code.* bus subscriptions for one chat. Returns the cleanup that
 * detaches any child-session listeners (called when code_task completes).
 */
export function useCodeFlow(
  dispatch: TranscriptDispatch,
  aliveRef: Ref<boolean>,
  codeActiveRef: Ref<boolean>,
): () => void {
  const childUnlistensRef = useRef<(() => void)[]>([]);

  /** Tool activity inside a code-harness CHILD session (plan/execute) —
   * rendered into this transcript so the code being written is visible live,
   * Cursor-style. Deltas/turn events of the child stay out (the harness's
   * final report arrives through code_task's own tool result). */
  const onChildEvent = useCallback(
    (event: DeaconEvent) => {
      if (!aliveRef.current) return;
      const p = event.params;
      if (event.method === 'tool.start' && typeof p.tool === 'string') {
        dispatch({
          type: 'tool-start',
          name: p.tool,
          detail: detailFromArgs(p.tool, p.args_summary),
          code: codeFromArgs(p.args_detail),
        });
      } else if (event.method === 'tool.complete' && typeof p.tool === 'string') {
        dispatch({
          type: 'tool-end',
          name: p.tool,
          isError: p.is_error === true,
          ...detailFromResult(p.result_summary),
        });
      } else if (event.method === 'turn.complete' && typeof p.error === 'string' && p.error !== '') {
        // A child turn died (provider error, interrupt) — show it, never
        // swallow it behind a spinner that just stops.
        dispatch({ type: 'notice', text: clip(p.error, 200), tone: 'warn' });
      }
    },
    [dispatch, aliveRef],
  );

  useEffect(() => {
    const s = t().chat.transcript;
    const unsubs = CODE_METHODS.map((method) =>
      subscribe({ method }, (event) => {
        if (!aliveRef.current || !codeActiveRef.current) return;
        const p = event.params;
        switch (event.method) {
          case 'code.planning':
          case 'code.started': {
            if (typeof p.session_id !== 'string') return;
            dispatch({
              type: 'notice',
              text: event.method === 'code.planning' ? s.codePlanning : s.codeExecuting,
              tone: 'ok',
            });
            childUnlistensRef.current.push(subscribe({ sessionId: p.session_id }, onChildEvent));
            break;
          }
          case 'code.verify': {
            const passed = p.passed === true;
            const summary = typeof p.summary === 'string' && p.summary !== '' ? ` — ${p.summary}` : '';
            dispatch({
              type: 'notice',
              text: `${passed ? s.verifyPassed : s.verifyFailed}${summary}`,
              tone: passed ? 'ok' : 'warn',
            });
            break;
          }
          case 'code.reverted':
            dispatch({ type: 'notice', text: s.codeReverted, tone: 'warn' });
            break;
          default:
            break;
        }
      }),
    );
    return () => {
      for (const unsub of unsubs) unsub();
      for (const unlisten of childUnlistensRef.current) unlisten();
      childUnlistensRef.current = [];
    };
  }, [onChildEvent, dispatch, aliveRef, codeActiveRef]);

  return useCallback(() => {
    for (const unlisten of childUnlistensRef.current) unlisten();
    childUnlistensRef.current = [];
  }, []);
}
