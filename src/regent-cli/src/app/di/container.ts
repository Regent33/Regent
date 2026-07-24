// Composition root: resolve the deacon, spawn+health-probe candidates, and hand
// back the wired RpcClient. The only place infrastructure is constructed
// (Section 8 — DI).
import { connectHealthyDeacon } from "@shared/infrastructure/deacon/connect.ts";
import { regentHome } from "@shared/infrastructure/deacon/locate.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import type { Result } from "@shared/kernel/result.ts";

export interface AppDeps {
  readonly client: IRpcClient;
  readonly home: string;
}

/** Build the app's dependencies for the active profile ("" = default home).
 *  Async because it spawns and health-probes each deacon candidate until one
 *  answers — a stale pinned binary no longer wins the CLI a dead pipe. */
export async function buildContainer(profile: string): Promise<Result<AppDeps>> {
  const home = regentHome(profile);
  const connected = await connectHealthyDeacon(home);
  if (!connected.ok) return connected;
  return { ok: true, value: { client: connected.value.client, home } };
}
