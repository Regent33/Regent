// `regent tools list|enable|disable`. `list` queries the deacon's catalog;
// enable/disable change `tools.disabled` through the deacon's validated config
// write (the deacon honors it at catalog-build time on the next run).
import { out, printError } from "@app/cli/runtime.ts";
import {
  explainConfigFailure,
  readConfigKey,
  setConfigKeys,
} from "@features/inspect/cli/deaconConfig.ts";
import { regentHome } from "@shared/infrastructure/deacon/locate.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";

export async function toolsListCommand(client: IRpcClient): Promise<number> {
  const res = await client.call<
    Array<{ name: string; description: string; toolset: string; enabled: boolean }>
  >("tools.list", {}, 30_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  for (const t of res.value) {
    const mark = t.enabled ? style.teal("●") : style.grey("○");
    out(`${mark} ${t.name.padEnd(22)} ${style.grey(t.description)}`);
  }
  return 0;
}

export function toolsSetCommand(
  profile: string,
  action: "enable" | "disable",
  name: string | undefined,
): number {
  if (!name) {
    printError(`usage: regent tools ${action} <tool>`);
    return 1;
  }
  const home = regentHome(profile);
  // Read through the deacon, not by parsing config.yaml here: an unreadable
  // file has to stop the command, and this used to swallow the parse error and
  // "start fresh", writing back a config with everything else deleted.
  const current = readConfigKey(home, "tools.disabled");
  if (current === undefined) {
    printError("cannot read the current tool list — `regent config validate` says why");
    return 1;
  }
  const set = new Set(Array.isArray(current) ? current.filter((x) => typeof x === "string") : []);
  if (action === "disable") set.add(name);
  else set.delete(name);

  const r = setConfigKeys(home, [["tools.disabled", [...set]]]);
  if (r.status !== "ok") {
    printError(explainConfigFailure(r));
    return 1;
  }
  out(action === "disable" ? `disabled ${style.teal(name)}` : `enabled ${style.teal(name)}`);
  out(style.grey("(applies on the next `regent` command — the deacon reloads config each run)"));
  return 0;
}
