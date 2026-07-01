// `regent agents mom run|create|list|remove` — Mixture-of-Models groups (§B).
// A group is N proposer model specs + an aggregator; `run` fans them out and
// returns the aggregator's synthesis (mom.run RPC). create/list/remove edit
// $REGENT_HOME/config.yaml's `mom` map directly (mirrors `providers`/`tools`);
// run talks to the deacon.
import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { parseFlags } from "@app/cli/args.ts";
import { out, printError, withClient } from "@app/cli/runtime.ts";
import { regentHome } from "@shared/infrastructure/deacon/locate.ts";
import { style } from "@shared/ui/style.ts";
import YAML from "yaml";

interface MomGroup {
  proposers?: string[];
  aggregator?: string;
  max_proposers?: number;
}

export async function momCommand(profile: string, args: string[]): Promise<number> {
  const [sub = "list", ...rest] = args;
  switch (sub) {
    case "run":
      return run(profile, rest);
    case "create":
    case "add":
      return create(profile, rest);
    case "list":
      return list(profile);
    case "remove":
    case "rm":
      return remove(profile, rest[0]);
    default:
      printError(`unknown mom subcommand: ${sub}`);
      out(
        'usage: agents mom [list | create <name> --proposers a,b,c --aggregator d [--max n] | run <name> "<brief>" | remove <name>]',
      );
      return 1;
  }
}

async function run(profile: string, rest: string[]): Promise<number> {
  const [name, ...briefParts] = rest;
  const brief = briefParts.join(" ").trim();
  if (!name || !brief) {
    printError('usage: agents mom run <name> "<brief>"');
    return 1;
  }
  return withClient(profile, async (client) => {
    out(style.grey(`running mom group ${name}…`));
    const res = await client.call<{ group: string; synthesis: string }>(
      "mom.run",
      { name, brief },
      180_000,
    );
    if (!res.ok) {
      printError(res.error.message);
      return 1;
    }
    out(res.value.synthesis);
    return 0;
  });
}

const FLAGS = {
  proposers: { type: "string", alias: "p" },
  aggregator: { type: "string", alias: "a" },
  max: { type: "string" },
} as const;

function create(profile: string, rest: string[]): number {
  const { values, positionals } = parseFlags(rest, FLAGS);
  const name = positionals[0];
  const proposers = str(values.proposers)
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
  const aggregator = str(values.aggregator);
  if (!name) {
    printError("usage: agents mom create <name> --proposers a,b,c --aggregator d [--max n]");
    return 1;
  }
  if (proposers.length === 0) {
    printError("--proposers a,b,c is required (model specs, e.g. groq/llama-3.3-70b)");
    return 1;
  }
  if (!aggregator) {
    printError("--aggregator <model spec> is required");
    return 1;
  }
  const group: MomGroup = { proposers, aggregator };
  const max = Number.parseInt(str(values.max), 10);
  if (Number.isFinite(max) && max > 0) group.max_proposers = max;

  const doc = loadConfig(profile);
  const mom =
    typeof doc.mom === "object" && doc.mom !== null ? (doc.mom as Record<string, unknown>) : {};
  mom[name] = group;
  doc.mom = mom;
  writeConfig(profile, doc);
  out(`${style.pass("✓")} created mom group ${style.teal(name)} (${proposers.length} proposers)`);
  out(style.grey("(applies on the next `regent` command — the deacon reloads config each run)"));
  return 0;
}

function list(profile: string): number {
  const doc = loadConfig(profile);
  const mom =
    typeof doc.mom === "object" && doc.mom !== null ? (doc.mom as Record<string, MomGroup>) : {};
  const names = Object.keys(mom);
  if (names.length === 0) {
    out(style.grey("no mom groups — agents mom create <name> --proposers a,b,c --aggregator d"));
    return 0;
  }
  out(style.heading(`MoM groups — ${names.length}`));
  for (const name of names.sort()) {
    const g = mom[name] ?? {};
    out(`${style.teal(name)}`);
    out(`  ${style.grey("proposers ")} ${(g.proposers ?? []).join(", ") || "—"}`);
    out(`  ${style.grey("aggregator")} ${g.aggregator ?? "—"}`);
  }
  return 0;
}

function remove(profile: string, name: string | undefined): number {
  if (!name) {
    printError("usage: agents mom remove <name>");
    return 1;
  }
  const doc = loadConfig(profile);
  const mom =
    typeof doc.mom === "object" && doc.mom !== null ? (doc.mom as Record<string, unknown>) : {};
  if (!(name in mom)) {
    out(style.grey(`no mom group '${name}'`));
    return 0;
  }
  delete mom[name];
  doc.mom = mom;
  writeConfig(profile, doc);
  out(`${style.pass("✓")} removed mom group ${style.teal(name)}`);
  return 0;
}

function loadConfig(profile: string): Record<string, unknown> {
  const path = join(regentHome(profile), "config.yaml");
  let doc: Record<string, unknown> = {};
  try {
    const parsed = YAML.parse(readFileSync(path, "utf8")) as unknown;
    if (parsed && typeof parsed === "object") doc = parsed as Record<string, unknown>;
  } catch {
    // no / invalid config.yaml — start fresh
  }
  if (doc._config_version === undefined) doc._config_version = 1;
  return doc;
}

function writeConfig(profile: string, doc: Record<string, unknown>): void {
  const home = regentHome(profile);
  mkdirSync(home, { recursive: true });
  const path = join(home, "config.yaml");
  const tmp = join(home, `config.yaml.tmp.${process.pid}`);
  writeFileSync(tmp, YAML.stringify(doc));
  renameSync(tmp, path);
}

const str = (v: string | boolean | undefined): string => (typeof v === "string" ? v : "");
