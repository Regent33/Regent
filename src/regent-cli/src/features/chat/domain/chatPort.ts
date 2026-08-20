// The chat feature's outbound port — what the viewmodel needs from the world,
// expressed without naming the transport. The RPC implementation lives in data/.
import type { QuestionnaireAnswer } from "@features/chat/domain/questionnaire.ts";
import type { RpcFailure, RpcNotification } from "@shared/kernel/contracts.ts";
import type { Result } from "@shared/kernel/result.ts";

export interface UnreportedJob {
  readonly id?: string;
  readonly label?: string;
  readonly state?: string;
  readonly delivered?: boolean;
}

export interface ChatPort {
  /** Submit a prompt; resolves when the turn ends (no client-side timeout). */
  submit(text: string): Promise<Result<unknown, RpcFailure>>;
  /** Interrupt the in-flight turn. */
  interrupt(): Promise<Result<unknown, RpcFailure>>;
  /** Answer a pending approval request. `feedback` rides a denial: the
   *  deny-reason for a tool gate, or the free-text answer to `ask_user`. */
  respondApproval(approved: boolean, feedback?: string): Promise<Result<unknown, RpcFailure>>;
  /** Answer a pending structured question card. Separate from `respondApproval`
   *  because the answer is typed: a JSON blob stuffed into `feedback` would
   *  reach the model as a string it has to re-parse. */
  respondQuestion(answer: QuestionnaireAnswer): Promise<Result<unknown, RpcFailure>>;
  /** Subscribe to deacon turn events; returns an unsubscribe function. */
  onEvent(handler: (event: RpcNotification) => void): () => void;
  /** Background jobs that finished while nobody was listening. `job.finished`
   *  is a best-effort push held in client state, so a restart loses it; the
   *  ledger keeps the news until a turn actually carries it. */
  unreportedJobs(): Promise<Result<UnreportedJob[], RpcFailure>>;
}
