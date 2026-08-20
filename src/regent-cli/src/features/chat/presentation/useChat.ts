import { COPY } from "@app/config/brand.ts";
import type { ChatPort } from "@features/chat/domain/chatPort.ts";
import type { Answer, Question, QuestionnaireAnswer } from "@features/chat/domain/questionnaire.ts";
import {
  applyAnswer,
  describeAnswer,
  nextUnanswered,
} from "@features/chat/domain/questionnaire.ts";
import { type ChatState, initialChatState, reduceChat } from "@features/chat/domain/transcript.ts";
// Chat viewmodel: subscribes deacon events into the transcript reducer and
// exposes the three user actions. All transcript mutation goes through the pure
// reducer; this hook only wires the port and dispatches.
import { useEffect, useReducer, useRef } from "react";

// Redrawing Ink's live region per streaming token thrashes the terminal (CPU +
// jank, and you can't stay scrolled up while it redraws). Coalesce delta text
// and flush at ~20fps; concatenated deltas reduce to the same state, so output
// is identical — just fewer frames.
const DELTA_FLUSH_MS = 50;

// Job ids already announced in this process. The live push and the replay are
// two routes to the same news, and a job finishing inside the one-round-trip
// window between subscribing and `job.list` returning arrives on BOTH — the
// reducer has no dedup of its own. Not the durable guard; that is the ledger's
// `delivered_at`, which stops the replay once a turn has carried the result.
const announced = new Set<string>();

/** True the first time this job id is seen, false every time after. */
function firstSighting(id: unknown): boolean {
  if (typeof id !== "string" || id === "") return true;
  if (announced.has(id)) return false;
  announced.add(id);
  return true;
}

export interface ChatViewModel {
  readonly state: ChatState;
  readonly sendPrompt: (text: string) => void;
  readonly interrupt: () => void;
  readonly respond: (approved: boolean, feedback?: string) => void;
  /** The question currently in front of the user, with its 1-based position. */
  readonly pendingQuestion: {
    readonly question: Question;
    readonly step: number;
    readonly total: number;
  } | null;
  /** Record an answer; submits the whole card once the last one is in. */
  readonly answerQuestion: (answer: Answer) => void;
  /** Dismiss the card — the model is told nothing was answered. */
  readonly cancelQuestion: () => void;
  /** Append a local note to the transcript (slash-command output). */
  readonly note: (text: string) => void;
  /** Clear the transcript (the `/new` command). */
  readonly reset: () => void;
}

export function useChat(port: ChatPort, sessionId: string): ChatViewModel {
  const [state, dispatch] = useReducer(reduceChat, initialChatState);
  const deltaBuf = useRef("");
  const flushTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    const flushDeltas = () => {
      if (flushTimer.current) {
        clearTimeout(flushTimer.current);
        flushTimer.current = null;
      }
      if (deltaBuf.current) {
        const text = deltaBuf.current;
        deltaBuf.current = "";
        dispatch({ type: "deaconEvent", method: "message.delta", params: { text } });
      }
    };

    const unsub = port.onEvent((event) => {
      const sid = event.params.session_id;
      if (typeof sid === "string" && sid !== sessionId) return; // ignore other sessions
      if (event.method === "message.delta") {
        deltaBuf.current += typeof event.params.text === "string" ? event.params.text : "";
        if (!flushTimer.current) flushTimer.current = setTimeout(flushDeltas, DELTA_FLUSH_MS);
        return;
      }
      // Any non-delta event (tool.start, message.complete, …) commits buffered
      // text first so transcript ordering is preserved.
      flushDeltas();
      // The push half of the dedup. Registering ids only on the replay path
      // left this asymmetric with the Desktop it was copied from, which routes
      // both routes through one helper (useJobNotices.ts announce()).
      if (event.method === "job.finished" && !firstSighting(event.params.job_id)) return;
      dispatch({ type: "deaconEvent", method: event.method, params: event.params });
    });

    // Replay what finished while nobody was listening. The push above is
    // best-effort and lives only in this process's state, so a restart — the
    // most likely thing to happen to a CLI — silently dropped the completion
    // the agent had promised to report. The ledger keeps it until a turn
    // actually carries it, so this self-terminates.
    let live = true;
    void port.unreportedJobs().then((res) => {
      if (!live || !res.ok || !Array.isArray(res.value)) return;
      for (const job of res.value) {
        if (job.delivered !== false) continue;
        if (job.state === "queued" || job.state === "running") continue;
        if (typeof job.label !== "string" || job.label === "") continue;
        if (!firstSighting(job.id)) continue;
        dispatch({
          type: "deaconEvent",
          method: "job.finished",
          params: { label: job.label, state: job.state ?? "finished", job_id: job.id },
        });
      }
    });

    return () => {
      live = false;
      if (flushTimer.current) clearTimeout(flushTimer.current);
      flushTimer.current = null;
      deltaBuf.current = "";
      unsub();
    };
  }, [port, sessionId]);

  const sendPrompt = (text: string) => {
    dispatch({ type: "userMessage", text });
    void port.submit(text).then((res) => {
      // Backstop: the reply streams via events; surface only an error they
      // didn't carry (mirrors the Go respMsg handling).
      if (!res.ok) dispatch({ type: "note", text: COPY.submitError(res.error.message) });
    });
  };

  const interrupt = () => {
    void port.interrupt();
  };

  const respond = (approved: boolean, feedback?: string) => {
    dispatch({ type: "approvalResolved", approved });
    void port.respondApproval(approved, feedback);
  };

  // Derived, not stored: one `question.request` carries every question, so the
  // stepper is "first one with no answer yet" rather than a counter that can
  // drift out of sync with the answers it indexes.
  const pending = state.question;
  const at = pending ? nextUnanswered(pending.questionnaire, pending.answers) : -1;
  const current = pending && at >= 0 ? pending.questionnaire.questions[at] : undefined;
  const pendingQuestion = current
    ? { question: current, step: at + 1, total: pending?.questionnaire.questions.length ?? 1 }
    : null;

  // Every answer arrives here — the reducer records it, and the port is called
  // exactly once, when the last question is answered. Answers are computed from
  // the state read at call time, so no dispatch-then-read staleness.
  const submit = (answers: QuestionnaireAnswer["answers"], cancelled: boolean) => {
    if (!pending) return;
    const lines = cancelled
      ? []
      : answers.map(([id, answer]) => {
          const q = pending.questionnaire.questions.find((x) => x.id === id);
          return q ? COPY.questionAnswer(q.prompt, describeAnswer(q, answer)) : "";
        });
    dispatch({ type: "questionResolved", lines: lines.filter(Boolean), cancelled });
    void port.respondQuestion({
      questionnaire_id: pending.questionnaire.id,
      answers,
      cancelled,
    });
  };

  const answerQuestion = (answer: Answer) => {
    if (!pending || !current) return;
    const answers = applyAnswer(pending.answers, current.id, answer);
    if (nextUnanswered(pending.questionnaire, answers) === -1) return submit(answers, false);
    dispatch({ type: "questionProgress", answers });
  };

  const cancelQuestion = () => submit([], true);

  const note = (text: string) => dispatch({ type: "note", text });
  const reset = () => dispatch({ type: "reset" });

  return {
    state,
    sendPrompt,
    interrupt,
    respond,
    pendingQuestion,
    answerQuestion,
    cancelQuestion,
    note,
    reset,
  };
}
