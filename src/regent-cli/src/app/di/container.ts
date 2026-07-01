import { locateDeacon, regentHome } from "@shared/infrastructure/deacon/locate.ts";
// Composition root: resolve the deacon, spawn it, and hand back the wired
// RpcClient. The only place infrastructure is constructed (Section 8 — DI).
import { connectDeacon } from "@shared/infrastructure/deacon/spawn.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import type { Result } from "@shared/kernel/result.ts";

export interface AppDeps {
  readonly client: IRpcClient;
  readonly home: string;
}

/** Build the app's dependencies for the active profile ("" = default home). */
export function buildContainer(profile: string): Result<AppDeps> {
  const located = locateDeacon();
  if (!located.ok) return located;

  const home = regentHome(profile);
  const connected = connectDeacon(located.value, home);
  if (!connected.ok) return connected;

  return { ok: true, value: { client: connected.value, home } };
}
