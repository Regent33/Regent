// The supported-provider catalog behind the setup wizard's pickers. Served by
// the deacon (`providers.catalog` — ProviderKind::ALL + curated models), so
// the CLI never carries a duplicate list that could drift.
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import type { Result } from "@shared/kernel/result.ts";

export interface ProviderInfo {
  readonly name: string;
  readonly key_env: string;
  readonly host: string;
  readonly needs_key: boolean;
  readonly models: readonly string[];
}

export async function fetchCatalog(
  client: IRpcClient,
): Promise<Result<readonly ProviderInfo[], Error>> {
  const res = await client.call<ProviderInfo[]>("providers.catalog", {}, 15_000);
  if (!res.ok) return { ok: false, error: new Error(res.error.message) };
  return { ok: true, value: res.value };
}
