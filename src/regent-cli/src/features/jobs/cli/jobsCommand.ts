// `regent jobs` / `regent jobs cancel <id>` — the background-job ledger's
// control surface (W9).
//
// The ledger has recorded jobs durably since W1 and has always been able to
// cancel one; nothing called it. A background task the user was told would
// "report back" ran its full 45-minute deadline with no way to stop it, and no
// way to even list what was running — job state only ever arrived bolted onto
// the user's next turn.
import { out, printError } from "@app/cli/runtime.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";
import { renderTable } from "@shared/ui/table.ts";
import { fmtTime } from "@shared/ui/time.ts";

interface JobRow {
  id: string;
  kind: string;
  label: string;
  state: string;
  session_id: string | null;
  deadline_at: number | null;
  cancel_requested: boolean;
  created_at: number;
}

export async function jobsCommand(client: IRpcClient, args: string[]): Promise<number> {
  if (args[0] === "cancel") {
    const id = args[1];
    if (!id) {
      printError("usage: regent jobs cancel <job-id>");
      return 1;
    }
    const res = await client.call<{ requested: boolean; note: string }>(
      "job.cancel",
      { id },
      30_000,
    );
    if (!res.ok) {
      printError(res.error.message);
      return 1;
    }
    // Not "cancelled": the runner notices on its next poll. Saying so is the
    // difference between a promise and a report.
    out(res.value.requested ? style.teal(res.value.note) : style.grey(res.value.note));
    return res.value.requested ? 0 : 1;
  }

  const res = await client.call<JobRow[]>("job.list", {}, 30_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  if (res.value.length === 0) {
    out(style.grey("no jobs running"));
    return 0;
  }
  out(style.heading(`Background jobs — ${res.value.length}`));
  for (const line of renderTable(res.value, [
    { header: "JOB", get: (j) => j.id, paint: (c) => style.teal(c) },
    { header: "KIND", get: (j) => j.kind },
    { header: "LABEL", get: (j) => j.label, flex: true },
    {
      header: "STATE",
      get: (j) => (j.cancel_requested ? `${j.state} (cancelling)` : j.state),
    },
    { header: "STARTED", get: (j) => fmtTime(j.created_at), paint: (c) => style.grey(c) },
    {
      header: "DEADLINE",
      get: (j) => (j.deadline_at ? fmtTime(j.deadline_at) : "-"),
      paint: (c) => style.grey(c),
    },
  ])) {
    out(line);
  }
  return 0;
}
