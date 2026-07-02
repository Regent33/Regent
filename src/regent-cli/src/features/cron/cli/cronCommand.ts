// `regent cron list|add|remove|autostart`. Mirrors cron.go.
import { spawnSync } from "node:child_process";
import { parseFlags } from "@app/cli/args.ts";
import { out, printError } from "@app/cli/runtime.ts";
import { locateDeacon } from "@shared/infrastructure/deacon/locate.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";
import { renderTable } from "@shared/ui/table.ts";

const fmtTime = (epoch: number): string =>
  new Date(epoch * 1000).toISOString().slice(0, 16).replace("T", " ");

const TASK_NAME = "RegentDeacon";

// Cron jobs only tick while a deacon runs; without this, closing the last chat
// (or rebooting) silently stops every scheduled task. Registers a logon task
// running the deacon in --keepalive mode; the cron tick lock prevents
// double-firing next to session-spawned deacons. Missed runs fire on the
// first tick after the deacon is back (lateness catch-up).
function cronAutostart(rest: string[]): number {
  if (process.platform !== "win32") {
    printError("autostart is Windows-only for now (schtasks) — use systemd/launchd manually.");
    return 1;
  }
  const run = (args: string[]) =>
    spawnSync("schtasks", args, { encoding: "utf8", windowsHide: true });
  if (rest[0] === "--remove") {
    const r = run(["/Delete", "/F", "/TN", TASK_NAME]);
    out(r.status === 0 ? `${style.pass("✓")} autostart removed` : (r.stderr ?? "").trim());
    return r.status ?? 1;
  }
  if (rest[0] === "--status") {
    const r = run(["/Query", "/TN", TASK_NAME]);
    out(
      r.status === 0
        ? `${style.pass("✓")} autostart installed (task ${TASK_NAME})`
        : "autostart not installed — regent cron autostart",
    );
    return 0;
  }
  const deacon = locateDeacon();
  if (!deacon.ok) {
    printError(
      "regent-deacon binary not found — build it first (cargo build -p regent-deacon --release).",
    );
    return 1;
  }
  const create = run([
    "/Create",
    "/F",
    "/SC",
    "ONLOGON",
    "/TN",
    TASK_NAME,
    "/TR",
    `"${deacon.value}" --keepalive`,
  ]);
  if (create.status !== 0) {
    printError((create.stderr ?? "schtasks create failed").trim());
    return 1;
  }
  run(["/Run", "/TN", TASK_NAME]); // start it now, not only at next logon
  out(
    `${style.pass("✓")} deacon autostart installed + started ${style.grey("(logon task; cron/schedules now fire without an open session)")}`,
  );
  out(style.grey("  remove with: regent cron autostart --remove"));
  return 0;
}

export async function cronCommand(client: IRpcClient, args: string[]): Promise<number> {
  const [sub, ...rest] = args;

  // Local (no deacon RPC): manage the Windows logon task.
  if (sub === "autostart") return cronAutostart(rest);

  if (sub === "add") {
    const { values, positionals } = parseFlags(rest, {
      schedule: { type: "string" },
      prompt: { type: "string" },
    });
    const name = positionals[0];
    if (!name || !values.schedule || !values.prompt) {
      printError("usage: regent cron add <name> --schedule <when> --prompt <text>");
      out(style.grey("  when: 30m · 2h · 1d (every N) · 'daily 09:30' · @<epoch> (one-shot)"));
      return 1;
    }
    const res = await client.call<{ id: string }>(
      "cron.add",
      { name, schedule: values.schedule, prompt: values.prompt },
      30_000,
    );
    if (!res.ok) {
      printError(res.error.message);
      if (/invalid schedule/i.test(res.error.message)) {
        out(style.grey("  when: 30m · 2h · 1d (every N) · 'daily 09:30' · @<epoch> (one-shot)"));
      }
      return 1;
    }
    out(`added ${style.teal(res.value.id)}`);
    return 0;
  }

  if (sub === "remove") {
    const id = rest[0];
    if (!id) {
      printError("usage: regent cron remove <job-id>");
      return 1;
    }
    const res = await client.call<{ removed: boolean }>("cron.remove", { id }, 30_000);
    if (!res.ok) {
      printError(res.error.message);
      return 1;
    }
    out(res.value.removed ? "removed" : style.grey("no job with that id"));
    return 0;
  }

  if (sub === "pause" || sub === "resume") {
    const id = rest[0];
    if (!id) {
      printError(`usage: regent cron ${sub} <job-id>`);
      return 1;
    }
    const enabled = sub === "resume";
    const res = await client.call<{ found: boolean }>("cron.set_enabled", { id, enabled }, 30_000);
    if (!res.ok) {
      printError(res.error.message);
      return 1;
    }
    out(res.value.found ? (enabled ? "resumed" : "paused") : style.grey("no job with that id"));
    return 0;
  }

  if (sub === "run") {
    const id = rest[0];
    if (!id) {
      printError("usage: regent cron run <job-id>");
      return 1;
    }
    const res = await client.call<{ queued: boolean }>("cron.run", { id }, 30_000);
    if (!res.ok) {
      printError(res.error.message);
      return 1;
    }
    out(
      res.value.queued
        ? "queued — runs on the next scheduler tick"
        : style.grey("no job with that id"),
    );
    return 0;
  }

  if (sub === "edit") {
    const { values, positionals } = parseFlags(rest, {
      name: { type: "string" },
      schedule: { type: "string" },
      prompt: { type: "string" },
    });
    const id = positionals[0];
    if (!id || (!values.name && !values.schedule && !values.prompt)) {
      printError(
        "usage: regent cron edit <job-id> [--name <n>] [--schedule <when>] [--prompt <text>]",
      );
      return 1;
    }
    const params: Record<string, unknown> = { id };
    if (values.name) params.name = values.name;
    if (values.schedule) params.schedule = values.schedule;
    if (values.prompt) params.prompt = values.prompt;
    const res = await client.call<{ updated: boolean }>("cron.edit", params, 30_000);
    if (!res.ok) {
      printError(res.error.message);
      return 1;
    }
    out(res.value.updated ? "updated" : style.grey("no job with that id"));
    return 0;
  }

  // Default: list.
  const res = await client.call<
    Array<{ id: string; name: string; prompt: string; enabled: boolean; next_run_at: number }>
  >("cron.list", {}, 30_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  if (res.value.length === 0) {
    out(style.grey("no cron jobs"));
    return 0;
  }
  out(style.heading(`Cron — ${res.value.length} job(s)`));
  for (const line of renderTable(res.value, [
    { header: "ID", get: (j) => j.id, paint: (c) => style.teal(c) },
    { header: "NAME", get: (j) => j.name },
    {
      header: "STATE",
      get: (j) => (j.enabled ? "enabled" : "disabled"),
      paint: (c, j) => (j.enabled ? style.pass(c) : style.grey(c)),
    },
    { header: "NEXT RUN", get: (j) => fmtTime(j.next_run_at) },
    { header: "PROMPT", get: (j) => j.prompt, flex: true, paint: (c) => style.grey(c) },
  ])) {
    out(line);
  }
  return 0;
}
