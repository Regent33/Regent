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

/** One request/response pair, for the workspace Debug Console. */
export interface RpcTraffic {
  readonly method: string;
  /** Round-trip in ms — the number that tells a slow deacon from a slow model. */
  readonly ms: number;
  readonly ok: boolean;
  readonly detail?: string;
}

// A tap, not a log: nothing is retained here. Without a subscriber this costs
// one null check per call, so the Debug Console pays for itself only while open.
type TrafficWatcher = (traffic: RpcTraffic) => void;
const watchers = new Set<TrafficWatcher>();

/** Watch request/response traffic. Returns an unsubscribe fn. */
export function onRpcTraffic(watcher: TrafficWatcher): () => void {
  watchers.add(watcher);
  return () => {
    watchers.delete(watcher);
  };
}

function report(traffic: RpcTraffic): void {
  for (const watcher of watchers) watcher(traffic);
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
  const started = performance.now();
  const elapsed = (): number => Math.round(performance.now() - started);
  let response: unknown;
  try {
    response = await invoke("deacon_request", { method, params });
  } catch (cause) {
    report({ method, ms: elapsed(), ok: false, detail: String(cause) });
    return err(failure("ipc", `deacon_request ${method}: ${String(cause)}`, cause));
  }
  const envelope = (response ?? {}) as RpcEnvelope<T>;
  if (envelope.error) {
    const detail = envelope.error.message ?? `deacon error on ${method}`;
    report({ method, ms: elapsed(), ok: false, detail });
    return err(failure("rpc", detail, envelope.error));
  }
  report({ method, ms: elapsed(), ok: true });
  return ok(envelope.result as T);
}

/** `deaconRequest` that rides out deacon startup: on first launch the webview
 * paints before the deacon finishes spawning, so initial loads (persona,
 * profiles) got one "ipc" failure and rendered an empty page forever. Retries
 * transport failures ("ipc" — bridge/deacon not up yet) with a fixed delay;
 * real JSON-RPC errors ("rpc") return immediately — the server answered. */
export async function deaconRequestRetry<T = unknown>(
  method: string,
  params: Record<string, unknown> = {},
  tries = 6,
  delayMs = 700,
): Promise<Result<T, Failure>> {
  let last: Result<T, Failure> = err(failure("ipc", `deacon_request ${method}: no attempt ran`));
  for (let attempt = 0; attempt < tries; attempt += 1) {
    last = await deaconRequest<T>(method, params);
    if (last.ok || last.error.kind !== "ipc") return last;
    await new Promise((resolve) => setTimeout(resolve, delayMs));
  }
  return last;
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
