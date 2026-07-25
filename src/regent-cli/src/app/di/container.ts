// Composition root: resolve the deacon, spawn+health-probe candidates, and hand
// back the wired RpcClient. The only place infrastructure is constructed
// (Section 8 — DI).
import { connectHealthyDeacon, withHealthRecovery } from "@shared/infrastructure/deacon/connect.ts";
import { deaconCandidates, regentHome } from "@shared/infrastructure/deacon/locate.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import type { Result } from "@shared/kernel/result.ts";

export interface AppDeps {
  readonly client: IRpcClient;
  readonly home: string;
}

/** Build the app's dependencies for the active profile ("" = default home).
 *  A second health check may recover once if the selected child dies just after
 *  its probe. The failed path is excluded so a flaky stale pin cannot win twice. */
export async function buildContainer(profile: string): Promise<Result<AppDeps>> {
  const home = regentHome(profile);
  const connected = await connectHealthyDeacon(home);
  if (!connected.ok) return connected;
  const client = withHealthRecovery(connected.value, (failedPath) =>
    connectHealthyDeacon(home, {
      candidates: deaconCandidates().filter((candidate) => candidate !== failedPath),
    }),
  );
  return { ok: true, value: { client, home } };
}
