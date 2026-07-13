// The chat feature's outbound port — what the viewmodel needs from the world,
// expressed without naming the transport. The RPC implementation lives in data/.
import type { RpcFailure, RpcNotification } from "@shared/kernel/contracts.ts";
import type { Result } from "@shared/kernel/result.ts";

export interface ChatPort {
  /** Submit a prompt; resolves when the turn ends (no client-side timeout). */
  submit(text: string): Promise<Result<unknown, RpcFailure>>;
  /** Interrupt the in-flight turn. */
  interrupt(): Promise<Result<unknown, RpcFailure>>;
  /** Answer a pending approval request. `feedback` rides a denial: the
   *  deny-reason for a tool gate, or the free-text answer to `ask_user`. */
  respondApproval(approved: boolean, feedback?: string): Promise<Result<unknown, RpcFailure>>;
  /** Subscribe to deacon turn events; returns an unsubscribe function. */
  onEvent(handler: (event: RpcNotification) => void): () => void;
}
