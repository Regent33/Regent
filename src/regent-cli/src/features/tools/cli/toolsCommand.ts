// `regent tools list|enable|disable`. `list` queries the deacon's catalog;
// enable/disable edit $REGENT_HOME/config.yaml's `tools.disabled` (filesystem —
// the deacon honors it at catalog-build time on the next run).
import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { out, printError } from "@app/cli/runtime.ts";
import { regentHome } from "@shared/infrastructure/deacon/locate.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";
import YAML from "yaml";

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
  const path = join(home, "config.yaml");

  let doc: Record<string, unknown> = {};
  try {
    const parsed = YAML.parse(readFileSync(path, "utf8")) as unknown;
    if (parsed && typeof parsed === "object") doc = parsed as Record<string, unknown>;
  } catch {
    // no / invalid config.yaml — start fresh
  }
  if (doc._config_version === undefined) doc._config_version = 1;

  const tools =
    typeof doc.tools === "object" && doc.tools !== null
      ? (doc.tools as Record<string, unknown>)
      : {};
  const current = Array.isArray(tools.disabled)
    ? (tools.disabled as unknown[]).filter((x): x is string => typeof x === "string")
    : [];
  const set = new Set(current);
  if (action === "disable") set.add(name);
  else set.delete(name);
  tools.disabled = [...set];
  doc.tools = tools;

  mkdirSync(home, { recursive: true });
  const tmp = join(home, `config.yaml.tmp.${process.pid}`);
  writeFileSync(tmp, YAML.stringify(doc));
  renameSync(tmp, path);

  out(action === "disable" ? `disabled ${style.teal(name)}` : `enabled ${style.teal(name)}`);
  out(style.grey("(applies on the next `regent` command — the deacon reloads config each run)"));
  return 0;
}
