// Spawn-and-health-probe fallback: try each deacon candidate in order and return
// the first that answers `health`. The reason this exists: a pinned but stale
// REGENT_DEACON_PATH (an old binary whose config schema drifted) boots then dies
// with a fatal line, and the CLI used to spawn only that first pick and surface a
// bare EPIPE / dead pipe. Now a dead candidate is closed and skipped, and if ALL
// fail the error lists every attempted path with its real reason.
import type { IRpcClient, RpcFailure, RpcNotification } from "@shared/kernel/contracts.ts";
import { err, failure, ok, type Result } from "@shared/kernel/result.ts";
import { deaconCandidates } from "./locate.ts";
import { ensureHome, type SpawnedDeacon, spawnDeaconChild } from "./spawn.ts";

// Health-probe ceiling per candidate. A healthy deacon answers in well under a
// second; a doomed one exits (stdout closes → the RpcClient rejects at once) or
// never answers. Generous for a cold, AV-scanned first spawn of a fresh build.
const PROBE_TIMEOUT_MS = 15_000;

export interface HealthyDeacon {
  readonly client: IRpcClient;
  /** The candidate that actually passed the health probe. */
  readonly path: string;
}

type SpawnFn = (path: string) => Result<SpawnedDeacon>;

// Injection seam for tests: override the candidate list and/or the spawn so the
// fallback loop can be driven with fake exiting/healthy children in memory.
export interface ConnectOptions {
  readonly candidates?: readonly string[];
  readonly spawn?: SpawnFn;
  readonly timeoutMs?: number;
}

/** Connect to the first healthy deacon among the resolved candidates. */
export async function connectHealthyDeacon(
  home: string,
  opts: ConnectOptions = {},
): Promise<Result<HealthyDeacon>> {
  const ready = ensureHome(home);
  if (!ready.ok) return ready;

  const candidates = opts.candidates ?? deaconCandidates();
  const spawnOne: SpawnFn = opts.spawn ?? ((path) => spawnDeaconChild(path, home));
  const timeout = opts.timeoutMs ?? PROBE_TIMEOUT_MS;

  if (candidates.length === 0) {
    return err(
      failure(
        "deacon-spawn",
        "regent-deacon not found (set REGENT_DEACON_PATH or build with `cargo build -p regent-deacon`)",
      ),
    );
  }

  const attempts: string[] = [];
  for (const path of candidates) {
    const spawned = spawnOne(path);
    if (!spawned.ok) {
      attempts.push(`${path}: ${spawned.error.message}`);
      continue;
    }
    const { client, earlyStderr } = spawned.value;
    const health = await client.call("health", {}, timeout);
    if (health.ok) return ok({ client, path });
    // Boots-then-dies / never-answers: tear the child down and record WHY, then
    // fall through to the next candidate (e.g. a newer repo build).
    await client.close();
    attempts.push(`${path}: ${earlyStderr() || health.error.message}`);
  }

  return err(
    failure(
      "deacon-spawn",
      `no working regent-deacon among ${candidates.length} candidate(s):\n  ${attempts.join("\n  ")}`,
    ),
  );
}

function isTransportFailure(f: RpcFailure): boolean {
  return f.code === undefined && !f.message.includes("timed out after");
}

const startupFailure = (): RpcFailure => ({
  kind: "rpc",
  message: "regent-deacon stopped during startup; run `regent doctor` for details",
});

/** Recover only the idempotent startup health check. Replaying arbitrary RPCs
 * could duplicate a prompt, session, config write, or tool action if the deacon
 * handled the request before its pipe closed. */
export function withHealthRecovery(
  initial: HealthyDeacon,
  reconnect: (failedPath: string) => Promise<Result<HealthyDeacon>>,
): IRpcClient {
  let client = initial.client;
  let path = initial.path;
  let closed = false;
  const subs = new Map<(n: RpcNotification) => void, () => void>();

  return {
    async call<T = unknown>(
      method: string,
      params: Record<string, unknown> = {},
      timeoutMs?: number,
    ): Promise<Result<T, RpcFailure>> {
      if (closed) return err({ kind: "rpc", message: `${method}: client is closed` });
      const first = await client.call<T>(method, params, timeoutMs);
      if (first.ok || method !== "health" || !isTransportFailure(first.error)) return first;

      await client.close().catch(() => {});
      const reconnected = await reconnect(path);
      if (closed) {
        if (reconnected.ok) await reconnected.value.client.close().catch(() => {});
        return err({ kind: "rpc", message: `${method}: client is closed` });
      }
      if (!reconnected.ok) return err({ kind: "rpc", message: reconnected.error.message });

      client = reconnected.value.client;
      path = reconnected.value.path;
      for (const [handler, unsubscribe] of subs) {
        unsubscribe();
        subs.set(handler, client.onNotification(handler));
      }
      const replay = await client.call<T>(method, params, timeoutMs);
      if (!replay.ok && isTransportFailure(replay.error)) {
        await client.close().catch(() => {});
        return err(startupFailure());
      }
      return replay;
    },

    onNotification(handler: (n: RpcNotification) => void): () => void {
      if (closed) return () => {};
      subs.set(handler, client.onNotification(handler));
      return () => {
        subs.get(handler)?.();
        subs.delete(handler);
      };
    },

    async close(): Promise<void> {
      if (closed) return;
      closed = true;
      for (const unsubscribe of subs.values()) unsubscribe();
      subs.clear();
      await client.close();
    },
  };
}
