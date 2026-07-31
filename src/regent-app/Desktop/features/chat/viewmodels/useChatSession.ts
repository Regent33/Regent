'use client';
// One chat session: lazy `session.create` on first submit (or `session.resume`
// + `session.history` seeding for an existing id — fired in parallel, with a
// `resuming` flag so the UI shows progress), wire events mapped into the pure
// transcript reducer. Events are the source of truth for turn state — the
// `prompt.submit` response only resolves when the whole turn ends, and its
// -32000 failures duplicate the `turn.complete {error}` notification, so
// those are ignored here. Mutating tool actions arrive as `approval.request`
// and MUST be answered (`approval.respond`) or the deacon denies at 120s.
// Event/history mapping helpers live in data/eventDetails.ts; the code_task
// follow-along lives in useCodeFlow.ts.
import { useCallback, useEffect, useReducer, useRef, useState } from 'react';
import { t } from '@/shared/i18n/t';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';
import { isLocalCommand, parseSlashCommand, runLocalCommand } from '@/features/chat/data/localCommands';
import { bargeNotice } from '@/features/chat/domain/promptQueue';
import { type DeaconEvent, subscribe } from '@/shared/state/deaconBus';
import { currentOpenFile } from '@/shared/state/openFile';
import { editorChipLabel } from '@/shared/kernel/promptDecorations';
import { type TranscriptState, emptyTranscript, reduceTranscript } from '@/shared/kernel/transcript';
import {
  type HistoryRow,
  codeFromArgs,
  detailFromArgs,
  detailFromResult,
  rowToItems,
  stageAttachments,
} from '@/features/chat/data/eventDetails';
import { useCodeFlow } from '@/features/chat/viewmodels/useCodeFlow';
import { useJobNotices } from '@/features/chat/viewmodels/useJobNotices';

export interface ChatSession {
  readonly state: TranscriptState;
  readonly resuming: boolean;
  /** The live session id once created/resumed (undefined before the first
   * submit on a brand-new chat) — lets surfaces like the composer's activity
   * timer subscribe to this session's turn activity on the shared bus. */
  readonly sessionId: string | undefined;
  readonly submit: (text: string, attachments?: readonly File[]) => void;
  /** `barge` marks an interruption made to send something else — it ends the
   * turn with a quiet acknowledgement instead of the backend's failure line. */
  readonly stop: (barge?: boolean) => void;
  readonly respondApproval: (approved: boolean) => void;
  /** Resolve (creating on first use) this chat's session. The coding panel
   * calls it with a folder when the user opens one before ever sending a
   * message; single-flighted with the composer's own create. */
  readonly ensureSession: (
    workspace?: string,
  ) => Promise<{ ok: true; id: string } | { ok: false; error: string }>;
}

const RPC_TURN_ERROR = -32000; // already delivered via turn.complete {error}

export function useChatSession(initialSessionId?: string): ChatSession {
  const [state, dispatch] = useReducer(reduceTranscript, emptyTranscript);
  const [resuming, setResuming] = useState(false);
  const [sessionId, setSessionId] = useState<string | undefined>(undefined);
  const sessionRef = useRef<string | undefined>(undefined);
  const unlistenRef = useRef<(() => void) | undefined>(undefined);
  const aliveRef = useRef(true);
  // In-flight `session.create` — a rapid double-send (two submits racing
  // before the first create resolves) must share ONE request, not each fire
  // their own and leave a duplicate, mostly-empty session behind.
  const createPromiseRef = useRef<Promise<{ id: string } | { error: string }> | undefined>(undefined);

  // Set by `stop(true)` and consumed by the next turn.interrupted, so the
  // event can tell a barge-in from a plain Stop; the counter rotates the
  // acknowledgement wording across a conversation.
  const bargeRef = useRef(false);
  const bargeCountRef = useRef(0);

  // True while THIS chat's code_task tool call is in flight — the window in
  // which global code.* events belong to this transcript.
  const codeActiveRef = useRef(false);
  const clearCodeChildren = useCodeFlow(dispatch, aliveRef, codeActiveRef);
  // A background job that finishes while nobody is typing announces itself.
  useJobNotices(dispatch, aliveRef);

  const onEvent = useCallback(
    (event: DeaconEvent) => {
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
          if (typeof p.tool === 'string') {
            if (p.tool === 'code_task') codeActiveRef.current = true;
            dispatch({
              type: 'tool-start',
              name: p.tool,
              detail: detailFromArgs(p.tool, p.args_summary),
              code: codeFromArgs(p.args_detail),
            });
          }
          break;
        case 'tool.complete':
          if (typeof p.tool === 'string') {
            if (p.tool === 'code_task') {
              codeActiveRef.current = false;
              // The harness's child sessions are done — stop following them.
              clearCodeChildren();
            }
            dispatch({
              type: 'tool-end',
              name: p.tool,
              isError: p.is_error === true,
              ...detailFromResult(p.result_summary),
            });
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
          // A stop that lost the race (the turn finished first) leaves the
          // barge flag armed — it must not colour a later, deliberate Stop.
          bargeRef.current = false;
          dispatch({
            type: 'ended',
            error: typeof p.error === 'string' ? p.error : undefined,
          });
          break;
        // Two different gestures arrive on one event. A BARGE-IN (typing over
        // the answer) is followed by a new message, so it ends with a quiet
        // acknowledgement rather than the backend's "core: interrupted" in red.
        // Plain Stop is not — nothing follows it, so the reason still shows.
        case 'turn.interrupted':
          if (bargeRef.current) {
            bargeRef.current = false;
            bargeCountRef.current += 1;
            dispatch({
              type: 'ended',
              notice: bargeNotice(t().chat.composer.interrupted, bargeCountRef.current - 1),
            });
          } else {
            dispatch({ type: 'ended', error: typeof p.error === 'string' ? p.error : undefined });
          }
          break;
        case 'deacon.exited':
          dispatch({ type: 'failed', message: 'The agent backend exited.' });
          break;
        default:
          break;
      }
    },
    [clearCodeChildren],
  );

  const attach = useCallback(
    async (id: string) => {
      sessionRef.current = id;
      setSessionId(id);
      unlistenRef.current?.();
      unlistenRef.current = subscribe({ sessionId: id }, onEvent);
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

  /// Resolve this chat's session, creating it on first use. Single-flighted:
  /// a rapid double-send (or a folder opened while a submit is in flight) must
  /// share ONE `session.create`, not leave a duplicate empty session behind.
  /// `workspace` only applies to the call that actually performs the create —
  /// a session's root is fixed at birth.
  const ensureSession = useCallback(
    async (workspace?: string): Promise<{ ok: true; id: string } | { ok: false; error: string }> => {
      const existing = sessionRef.current;
      if (existing !== undefined) return { ok: true, id: existing };
      if (createPromiseRef.current === undefined) {
        createPromiseRef.current = (async (): Promise<{ id: string } | { error: string }> => {
          const created = await deaconRequest<{ session_id?: string }>(
            'session.create',
            workspace === undefined ? {} : { workspace },
          );
          if (!created.ok || typeof created.value?.session_id !== 'string') {
            return { error: created.ok ? 'session.create returned no id' : created.error.message };
          }
          await attach(created.value.session_id);
          return { id: created.value.session_id };
        })().finally(() => {
          createPromiseRef.current = undefined;
        });
      }
      const result = await createPromiseRef.current;
      return 'error' in result ? { ok: false, error: result.error } : { ok: true, id: result.id };
    },
    [attach],
  );

  const submit = useCallback(
    (text: string, attachments?: readonly File[]) => {
      // CLI-parity slash commands (`/status`, `/model`, …) execute as direct
      // RPCs and render locally — they never enter the model pipeline.
      const cmd = parseSlashCommand(text);
      if (cmd !== undefined && isLocalCommand(cmd.name)) {
        dispatch({ type: 'submitted', text });
        void runLocalCommand(cmd).then((reply) => {
          if (!aliveRef.current) return;
          dispatch({ type: 'reply', text: reply });
          dispatch({ type: 'ended' });
        });
        return;
      }
      // Echo the user's message (and the pending dots via busy) IMMEDIATELY —
      // session.create builds the whole agent and can take seconds; the
      // submit must never feel dead while that runs. Attachment names and the
      // editor context ride along as chips, so the message shows what it
      // carried before staging has even finished uploading anything — and
      // matches what a reload rebuilds from the stored prompt.
      const opened = currentOpenFile();
      const chipContext = opened.enabled ? editorChipLabel(opened) : undefined;
      dispatch({
        type: 'submitted',
        text,
        ...(attachments !== undefined && attachments.length > 0
          ? { attachments: attachments.map((file) => file.name) }
          : {}),
        ...(chipContext === undefined ? {} : { context: chipContext }),
      });
      void (async () => {
        const created = await ensureSession();
        if (!aliveRef.current) return;
        if (!created.ok) {
          dispatch({ type: 'failed', message: created.error });
          return;
        }
        const sessionId = created.id;
        const staged = await stageAttachments(sessionId, attachments ?? [], () => aliveRef.current);
        if (staged === null) return;
        if ('error' in staged) {
          dispatch({ type: 'failed', message: staged.error });
          return;
        }
        // What the coding panel has open travels with the turn, so the agent
        // works on the file the user is actually looking at instead of asking
        // which one they meant. Read at SUBMIT time, not render time — the
        // user may open a different file while typing.
        // The chip above the composer is the switch: off means nothing from
        // the panel travels, however much is open.
        const editor = currentOpenFile();
        const share = editor.enabled;
        const result = await deaconRequest('prompt.submit', {
          session_id: sessionId,
          text,
          ...(staged.paths.length > 0 ? { attachments: staged.paths } : {}),
          ...(share && editor.path !== undefined ? { open_file: editor.path } : {}),
          ...(share && editor.path === undefined && editor.folder !== undefined
            ? { open_folder: editor.folder }
            : {}),
          ...(share && editor.selection !== undefined
            ? {
                selection: {
                  start_line: editor.selection.startLine,
                  end_line: editor.selection.endLine,
                  text: editor.selection.text,
                },
              }
            : {}),
        });
        if (!aliveRef.current || result.ok) return;
        const code = (result.error.cause as { code?: number } | undefined)?.code;
        // rpc turn failures arrived as turn.complete {error}; report the rest
        // (bridge dead, boundary rejection) since no event will follow.
        if (result.error.kind !== 'rpc' || code !== RPC_TURN_ERROR) {
          dispatch({ type: 'failed', message: result.error.message });
        }
      })();
    },
    [ensureSession],
  );

  const stop = useCallback((barge?: boolean) => {
    const sessionId = sessionRef.current;
    if (sessionId === undefined) return;
    bargeRef.current = barge === true;
    void deaconRequest('turn.interrupt', { session_id: sessionId });
  }, []);

  const respondApproval = useCallback((approved: boolean) => {
    const sessionId = sessionRef.current;
    if (sessionId === undefined) return;
    dispatch({ type: 'approval-resolved', approved });
    void deaconRequest('approval.respond', { session_id: sessionId, approved });
  }, []);

  return { state, resuming, sessionId, submit, stop, respondApproval, ensureSession };
}
