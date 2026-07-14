// `regent code settings` — the CLI's code-settings surface, mirroring the
// desktop app's Settings → Code page. Bare: show the code-related settings and
// their current values (read via `config.get`). `auto on|off` flips
// `tools.auto_approve` through the deacon's validated `config.set` path (the
// whole file is schema-checked before the write, and the change applies live —
// open sessions too), never by freehand YAML edits.
import { out, printError, withClient } from "@app/cli/runtime.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";

const USAGE = "usage: regent code settings [auto on|off]";

/** The slice of `config.get` this surface reads. */
interface CodeConfig {
  tools?: { auto_approve?: boolean };
}

export async function codeSettingsCommand(
  profile: string,
  args: readonly string[],
): Promise<number> {
  const [sub, value, ...extra] = args;

  if (sub === undefined) {
    return withClient(profile, (client) => showSettings(client));
  }
  if (sub === "auto" && (value === "on" || value === "off") && extra.length === 0) {
    return withClient(profile, (client) => setAuto(client, value === "on"));
  }
  printError(USAGE);
  return 1;
}

async function showSettings(client: IRpcClient): Promise<number> {
  const res = await client.call<CodeConfig>("config.get", {}, 30_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  const on = res.value.tools?.auto_approve === true;
  out(style.heading("Code settings"));
  out(
    `  ${style.teal("auto-approve".padEnd(14))} ${style.value(on ? "on" : "off")}  ${style.grey(
      "— approve every tool gate without prompting (applies live, open sessions too)",
    )}`,
  );
  out(style.grey("\nchange with: regent code settings auto on|off"));
  return 0;
}

async function setAuto(client: IRpcClient, enabled: boolean): Promise<number> {
  const res = await client.call<{ changed: string; note: string }>(
    "config.set",
    { path: "tools.auto_approve", value: enabled },
    30_000,
  );
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  out(`auto-approve ${style.value(enabled ? "on" : "off")}`);
  out(style.grey(`(${res.value.note})`));
  return 0;
}
