// Kernel contracts — the interfaces any layer may depend on. Concrete
// implementations live in shared/infrastructure and are wired via DI.
import type { Failure, Result } from "./result.ts";

/** A server-initiated event (turn.started, tool.start, message.delta, …). */
export interface RpcNotification {
  readonly method: string;
  readonly params: Record<string, unknown>;
}

/** Typed failure raised by the JSON-RPC transport. */
export interface RpcFailure extends Failure {
  readonly kind: "rpc";
  /** JSON-RPC error code when the deacon answered with an error object. */
  readonly code?: number;
}

/**
 * Speaks newline-delimited JSON-RPC 2.0 to the deacon. Responses resolve by
 * id; notifications fan out to every registered handler. Mirrors the Go
 * client's contract (rpc.Client) so both front-ends share one protocol.
 */
export interface IRpcClient {
  /** Send a request and await its response. Resolves to a typed failure on error/timeout. */
  call<T = unknown>(
    method: string,
    params?: Record<string, unknown>,
    timeoutMs?: number,
  ): Promise<Result<T, RpcFailure>>;

  /** Subscribe to deacon notifications. Returns an unsubscribe function. */
  onNotification(handler: (n: RpcNotification) => void): () => void;

  /** Close the transport; a spawned deacon sees EOF and drains. */
  close(): Promise<void>;
}
