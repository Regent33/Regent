import { locateDaemon, regentHome } from "@shared/infrastructure/daemon/locate.ts";
// Composition root: resolve the daemon, spawn it, and hand back the wired
// RpcClient. The only place infrastructure is constructed (Section 8 — DI).
import { connectDaemon } from "@shared/infrastructure/daemon/spawn.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import type { Result } from "@shared/kernel/result.ts";

export interface AppDeps {
  readonly client: IRpcClient;
  readonly home: string;
}

/** Build the app's dependencies for the active profile ("" = default home). */
export function buildContainer(profile: string): Result<AppDeps> {
  const located = locateDaemon();
  if (!located.ok) return located;

  const home = regentHome(profile);
  const connected = connectDaemon(located.value, home);
  if (!connected.ok) return connected;

  return { ok: true, value: { client: connected.value, home } };
}
