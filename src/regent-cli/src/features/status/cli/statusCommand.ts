// `regent status` — a compact deacon health/state snapshot (status.get).
import { out, printError } from "@app/cli/runtime.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { displayModel } from "@shared/ui/displayModel.ts";
import { style } from "@shared/ui/style.ts";
import { fmtTime } from "@shared/ui/time.ts";

interface Status {
  model: string;
  active_sessions: number;
  cron: { jobs: number; enabled: number; next_run_at: number | null } | null;
}

export async function statusCommand(client: IRpcClient): Promise<number> {
  const res = await client.call<Status>("status.get", {}, 15_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  const s = res.value;
  out(style.heading("Regent status"));
  out(`  ${"model".padEnd(16)} ${style.value(displayModel(s.model))}`);
  out(`  ${"active sessions".padEnd(16)} ${String(s.active_sessions)}`);
  if (s.cron) {
    const next = s.cron.next_run_at ? `  (next ${fmtTime(s.cron.next_run_at)})` : "";
    out(`  ${"cron jobs".padEnd(16)} ${s.cron.enabled}/${s.cron.jobs} enabled${next}`);
  }
  return 0;
}
