// `regent agents mom run|create|list|remove` — Mixture-of-Models groups (§B).
// A group is N proposer model specs + an aggregator; `run` fans them out and
// returns the aggregator's synthesis (mom.run RPC). create/remove change
// `mom.<name>` through the deacon's validated config write (mirrors
// `providers`); `list` reads config.yaml; `run` talks to the deacon.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { parseFlags } from "@app/cli/args.ts";
import { out, printError, withClient } from "@app/cli/runtime.ts";
import {
  explainConfigFailure,
  runDeaconConfig,
  setConfigKeys,
} from "@features/inspect/cli/deaconConfig.ts";
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
  // One segment of a dotted config path — see the same guard in providers.
  if (name.includes(".")) {
    printError("a mom group name cannot contain '.'");
    return 1;
  }
  const group: MomGroup = { proposers, aggregator };
  const max = Number.parseInt(str(values.max), 10);
  if (Number.isFinite(max) && max > 0) group.max_proposers = max;

  const r = setConfigKeys(regentHome(profile), [[`mom.${name}`, group]]);
  if (r.status !== "ok") {
    printError(explainConfigFailure(r));
    return 1;
  }
  out(`${style.pass("✓")} created mom group ${style.teal(name)} (${proposers.length} proposers)`);
  out(style.grey("(applies on the next `regent` command — the deacon reloads config each run)"));
  return 0;
}

function list(profile: string): number {
  const mom = readGroups(profile);
  if (mom === null) return 1;
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
  // Without this, `mom remove panel.proposers` unsets a FIELD of the panel
  // group rather than reporting that there is no group by that name.
  if (name.includes(".")) {
    printError("a mom group name cannot contain '.'");
    return 1;
  }
  const r = runDeaconConfig(profile, ["unset", `mom.${name}`]);
  if (r.status === "not_set") {
    out(style.grey(`no mom group '${name}'`));
    return 0;
  }
  if (r.status !== "ok") {
    printError(explainConfigFailure(r));
    return 1;
  }
  out(`${style.pass("✓")} removed mom group ${style.teal(name)}`);
  return 0;
}

/**
 * The `mom` map from config.yaml, or null when the file cannot be read. A parse
 * error used to be swallowed and reported as "no mom groups", which reads as an
 * empty config rather than a broken one.
 */
function readGroups(profile: string): Record<string, MomGroup> | null {
  let raw: string;
  try {
    raw = readFileSync(join(regentHome(profile), "config.yaml"), "utf8");
  } catch {
    return {}; // no config yet — genuinely no groups
  }
  try {
    const doc = YAML.parse(raw) as Record<string, unknown> | null;
    const mom = doc?.mom;
    return typeof mom === "object" && mom !== null ? (mom as Record<string, MomGroup>) : {};
  } catch (e) {
    printError(`config.yaml is not valid YAML: ${(e as Error).message}`);
    return null;
  }
}

const str = (v: string | boolean | undefined): string => (typeof v === "string" ? v : "");
