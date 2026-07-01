// `regent config set <key> <value>` — edit $REGENT_HOME/config.yaml in place
// (dotted key path), atomic write. No deacon: each `regent` command spawns a
// fresh deacon that reloads config, so the change takes effect next run.
import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { out, printError } from "@app/cli/runtime.ts";
import { regentHome } from "@shared/infrastructure/deacon/locate.ts";
import { style } from "@shared/ui/style.ts";
import YAML from "yaml";

// Coerce a CLI string to a YAML scalar: booleans and plain numbers get typed.
function coerce(value: string): string | number | boolean {
  if (value === "true") return true;
  if (value === "false") return false;
  if (/^-?\d+(\.\d+)?$/.test(value)) return Number(value);
  return value;
}

function setDotted(root: Record<string, unknown>, dotted: string, value: unknown): void {
  const keys = dotted.split(".");
  let node = root;
  for (let i = 0; i < keys.length - 1; i++) {
    const k = keys[i] as string;
    if (typeof node[k] !== "object" || node[k] === null) node[k] = {};
    node = node[k] as Record<string, unknown>;
  }
  node[keys[keys.length - 1] as string] = value;
}

export function configSetCommand(profile: string, args: string[]): number {
  const [key, ...valueParts] = args;
  if (!key || valueParts.length === 0) {
    printError("usage: regent config set <key> <value>   (e.g. model.default claude-opus-4-8)");
    return 1;
  }
  const value = valueParts.join(" ");
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
  setDotted(doc, key, coerce(value));

  mkdirSync(home, { recursive: true });
  const tmp = join(home, `config.yaml.tmp.${process.pid}`);
  writeFileSync(tmp, YAML.stringify(doc));
  renameSync(tmp, path);

  out(`set ${style.teal(key)} = ${style.value(value)}`);
  out(style.grey("(applies on the next `regent` command — the deacon reloads config each run)"));
  return 0;
}
