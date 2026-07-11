'use client';
// The regent-code harness binding: `code.plan {task}` → review → approve →
// `code.start {task, plan}` (streams through the same session event path as
// chat — deltas, tool rows, approvals — into the shared transcript reducer),
// resolving {report, verify, reverted} at the end. Both methods already have
// turn-length bridge timeouts (630s).
import { useCallback, useEffect, useReducer, useRef, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';
import { type DeaconEvent, subscribe } from '@/shared/state/deaconBus';
import { emptyTranscript, reduceTranscript } from '@/shared/kernel/transcript';
import type { TranscriptState } from '@/shared/kernel/transcript';

export type CodePhase = 'idle' | 'planning' | 'plan-ready' | 'running' | 'done';

export interface CodeVerify {
  readonly passed: boolean;
  readonly summary: string;
}

export interface CodeRun {
  readonly phase: CodePhase;
  readonly task: string;
  readonly plan: string;
  readonly log: TranscriptState;
  readonly report?: string;
  readonly verify?: CodeVerify;
  readonly reverted: boolean;
  readonly error?: string;
  readonly makePlan: (task: string) => void;
  readonly approveRun: () => void;
  readonly discard: () => void;
  readonly stop: () => void;
  readonly respondApproval: (approved: boolean) => void;
}

interface PlanResult {
  readonly session_id?: string;
  readonly plan?: string;
}

interface StartResult {
  readonly report?: string;
  readonly verify?: { readonly passed?: boolean; readonly summary?: string } | null;
  readonly reverted?: boolean;
}

export function useCodeRun(): CodeRun {
  const [phase, setPhase] = useState<CodePhase>('idle');
  const [task, setTask] = useState('');
  const [plan, setPlan] = useState('');
  const [report, setReport] = useState<string | undefined>(undefined);
  const [verify, setVerify] = useState<CodeVerify | undefined>(undefined);
  const [reverted, setReverted] = useState(false);
  const [error, setError] = useState<string | undefined>(undefined);
  const [log, dispatch] = useReducer(reduceTranscript, emptyTranscript);
  const sessionRef = useRef<string | undefined>(undefined);
  const unlistenRef = useRef<(() => void) | undefined>(undefined);
  const aliveRef = useRef(true);

  useEffect(() => {
    aliveRef.current = true;
    return () => {
      aliveRef.current = false;
      unlistenRef.current?.();
    };
  }, []);

  const onEvent = useCallback((event: DeaconEvent) => {
    if (!aliveRef.current) return;
    const p = event.params;
    switch (event.method) {
      case 'message.delta':
        if (typeof p.text === 'string') dispatch({ type: 'delta', text: p.text });
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
      case 'turn.interrupted':
        dispatch({ type: 'ended', error: typeof p.error === 'string' ? p.error : undefined });
        break;
      default:
        // message.complete / turn.complete: the run's outcome comes from the
        // code.start response (report/verify), not per-turn events.
        break;
    }
  }, []);

  const makePlan = useCallback(
    (nextTask: string) => {
      if (!isTauri() || nextTask.trim() === '') return;
      setTask(nextTask);
      setError(undefined);
      setPhase('planning');
      void (async () => {
        const result = await deaconRequest<PlanResult>('code.plan', { task: nextTask });
        if (!aliveRef.current) return;
        if (!result.ok || typeof result.value?.plan !== 'string') {
          setError(result.ok ? 'code.plan returned no plan' : result.error.message);
          setPhase('idle');
          return;
        }
        if (typeof result.value.session_id === 'string') {
          sessionRef.current = result.value.session_id;
          unlistenRef.current?.();
          unlistenRef.current = subscribe({ sessionId: result.value.session_id }, onEvent);
        }
        setPlan(result.value.plan);
        setPhase('plan-ready');
      })();
    },
    [onEvent],
  );

  const approveRun = useCallback(() => {
    setPhase('running');
    // The run executes in a NEW session (the plan session is read-only), so
    // Stop / approvals / event streaming must rebind the moment the deacon
    // announces it — the code.start RESPONSE only arrives when the run ends.
    const unlistenStarted = subscribe({ method: 'code.started' }, (event) => {
      const sid = event.params.session_id;
      if (typeof sid === 'string') {
        sessionRef.current = sid;
        unlistenRef.current?.();
        unlistenRef.current = subscribe({ sessionId: sid }, onEvent);
      }
      unlistenStarted();
    });
    void (async () => {
      const result = await deaconRequest<StartResult>('code.start', { task, plan });
      unlistenStarted();
      if (!aliveRef.current) return;
      if (!result.ok) {
        setError(result.error.message);
        setPhase('done');
        return;
      }
      const v = result.value?.verify;
      setReport(typeof result.value?.report === 'string' ? result.value.report : undefined);
      setVerify(
        v && typeof v.passed === 'boolean'
          ? { passed: v.passed, summary: typeof v.summary === 'string' ? v.summary : '' }
          : undefined,
      );
      setReverted(result.value?.reverted === true);
      setPhase('done');
    })();
  }, [task, plan, onEvent]);

  const discard = useCallback(() => {
    unlistenRef.current?.();
    unlistenRef.current = undefined;
    sessionRef.current = undefined;
    setPlan('');
    setReport(undefined);
    setVerify(undefined);
    setReverted(false);
    setError(undefined);
    dispatch({ type: 'reset' });
    setPhase('idle');
  }, []);

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

  return {
    phase,
    task,
    plan,
    log,
    report,
    verify,
    reverted,
    error,
    makePlan,
    approveRun,
    discard,
    stop,
    respondApproval,
  };
}
