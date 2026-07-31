// The headless run, as a pure state machine over deacon notifications.
//
// Keeping it pure is what makes the machine contract testable: exit codes,
// stream discipline and the terminal event are the public API the moment
// `--events` ships, and none of them should need a live provider to verify.

/** A small shell taxonomy. Detail lives in the terminal event, not the code. */
export const ASK_EXIT = {
  ok: 0,
  /** Execution or runtime failure. */
  failure: 1,
  /** Usage or local validation. */
  usage: 2,
  /** Policy or budget prevented completion. */
  policy: 3,
  /** Session conflict, or a required resource was unavailable. */
  unavailable: 4,
  interrupted: 130,
} as const;

export type AskStatus =
  /** No terminal event has arrived yet. Never an exit status. */
  "running" | "completed" | "denied_by_policy" | "timed_out" | "interrupted" | "failed";

export interface AskState {
  /** The answer so far. */
  readonly answer: string;
  readonly done: boolean;
  readonly status: AskStatus;
  /** Tools refused because this run is not `--yes`. */
  readonly deniedApprovals: number;
  readonly toolCalls: number;
  /** An approval the run must answer, or null. */
  readonly pendingApproval: { readonly tool: string; readonly action: string } | null;
}

export const ASK_INITIAL: AskState = {
  answer: "",
  done: false,
  // Starts as "running", NOT "completed": a run that dies, disconnects or hangs
  // must not inherit success from its initial value and exit 0.
  status: "running",
  deniedApprovals: 0,
  toolCalls: 0,
  pendingApproval: null,
};

const str = (p: Record<string, unknown>, k: string): string =>
  typeof p[k] === "string" ? (p[k] as string) : "";

/**
 * Fold one deacon notification into the run state. The events are the same ones
 * the interactive surface consumes — headless is a different renderer, not a
 * different protocol.
 */
export function reduceAsk(s: AskState, method: string, params: Record<string, unknown>): AskState {
  switch (method) {
    case "message.delta":
      return { ...s, answer: s.answer + str(params, "text") };
    case "message.complete": {
      // The deacon sends the authoritative reply here; the deltas were a
      // preview of the same text, so replace rather than append.
      const reply = str(params, "reply");
      return reply ? { ...s, answer: reply } : s;
    }
    case "tool.start":
      return { ...s, toolCalls: s.toolCalls + 1 };
    case "approval.request":
      return {
        ...s,
        pendingApproval: { tool: str(params, "tool"), action: str(params, "action") },
      };
    case "turn.interrupted":
      return { ...s, done: true, status: "interrupted" };
    case "turn.complete":
      return { ...s, done: true, status: s.status === "running" ? "completed" : s.status };
    default:
      return s;
  }
}

/** Record that a pending approval was answered. */
export function resolveApproval(s: AskState, approved: boolean): AskState {
  return {
    ...s,
    pendingApproval: null,
    deniedApprovals: approved ? s.deniedApprovals : s.deniedApprovals + 1,
  };
}

/**
 * The exit code for a finished run.
 *
 * A denied approval is NOT a failed run: a run that is refused a write and still
 * answers has done exactly what a read-only automation run is supposed to do.
 * The first draft of the plan conflated the two; the outcome lives in the
 * terminal event instead.
 */
export function askExitCode(s: AskState): number {
  switch (s.status) {
    // A run that never reached a terminal event did not succeed, whatever else
    // happened — the deacon died, the pipe closed, or we stopped listening.
    case "running":
      return ASK_EXIT.failure;
    case "interrupted":
      return ASK_EXIT.interrupted;
    case "timed_out":
      return ASK_EXIT.policy;
    case "denied_by_policy":
      return ASK_EXIT.policy;
    case "failed":
      return ASK_EXIT.failure;
    default:
      return ASK_EXIT.ok;
  }
}

/** The terminal NDJSON event — the real outcome, for automation. */
export function terminalEvent(
  s: AskState,
  ids: { readonly runId: string; readonly sessionId: string },
): Record<string, unknown> {
  return {
    type: "run.completed",
    schema_version: 1,
    status: s.status,
    // Distinct from `status`: a run can complete with an answer that was cut
    // short, and a script needs to know which it got.
    answer_complete: s.status === "completed",
    denied_approvals: s.deniedApprovals,
    tool_calls: s.toolCalls,
    run_id: ids.runId,
    session_id: ids.sessionId,
  };
}
