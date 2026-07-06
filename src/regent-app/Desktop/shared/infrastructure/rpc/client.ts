// Webview side of the single IPC seam: typed requests to the deacon over
// Tauri `invoke`, streamed notifications over the `deacon-event` channel.
// The Rust bridge (src-tauri/src/commands.rs) validates and forwards; this
// wrapper unwraps the JSON-RPC envelope into the kernel Result so callers
// never parse envelopes. Outside the desktop shell (plain `next dev`, static
// prerender) every call fails typed / no-ops — the UI degrades gracefully.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { type Failure, type Result, err, failure, ok } from "@/shared/kernel/result";

export const isTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** A streamed deacon notification (one JSON-RPC notification line). Events
 * carry `session_id` in params — always filter on it (see task plan: a
 * background job's deltas must never render into a foreign session). */
export interface DeaconEvent {
  readonly method: string;
  readonly params: { readonly session_id?: string } & Record<string, unknown>;
}

interface RpcEnvelope<T> {
  readonly result?: T;
  readonly error?: { readonly code?: number; readonly message?: string };
}

/** Request/response against the deacon. Provider errors (401/402/429) arrive
 * as JSON-RPC errors — surfaced verbatim in the Failure, never masked. */
export async function deaconRequest<T = unknown>(
  method: string,
  params: Record<string, unknown> = {},
): Promise<Result<T, Failure>> {
  if (!isTauri()) {
    return err(failure("no-shell", "not running inside the desktop shell"));
  }
  let response: unknown;
  try {
    response = await invoke("deacon_request", { method, params });
  } catch (cause) {
    return err(failure("ipc", `deacon_request ${method}: ${String(cause)}`, cause));
  }
  const envelope = (response ?? {}) as RpcEnvelope<T>;
  if (envelope.error) {
    return err(failure("rpc", envelope.error.message ?? `deacon error on ${method}`, envelope.error));
  }
  return ok(envelope.result as T);
}

/** Subscribe to streamed deacon notifications. With `sessionId`, events from
 * other sessions are dropped; events without a session_id (global notices)
 * always pass. Returns an unlisten fn (no-op outside the shell). */
export async function onDeaconEvent(
  handler: (event: DeaconEvent) => void,
  sessionId?: string,
): Promise<UnlistenFn> {
  if (!isTauri()) return () => {};
  return listen<DeaconEvent>("deacon-event", ({ payload }) => {
    if (!payload || typeof payload.method !== "string") return;
    const sid = payload.params?.session_id;
    if (sessionId !== undefined && sid !== undefined && sid !== sessionId) return;
    handler(payload);
  });
}
