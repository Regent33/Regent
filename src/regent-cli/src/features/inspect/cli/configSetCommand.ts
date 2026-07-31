// `regent config set <key> <value>` — the only write path.
//
// The CLI used to hand-edit the YAML, so a typo like `moddel.default` was
// written happily and bricked the next launch. Now the change is always made by
// Rust, which proves the WHOLE file still deserialises into the real
// `DeaconConfig` (deny_unknown_fields + the provider enum) before replacing it,
// under a lock, atomically. Two ways in, one implementation:
//
//   deacon running  → the `config.set` RPC, so OPEN sessions pick it up live
//   deacon down     → `regent-deacon config set`, a short-lived process
//
// The second is the case that matters: a config bad enough to stop the daemon is
// exactly when you need to edit it, and it is still fully validated.
import { parseFlags, unknownFlags } from "@app/cli/args.ts";
import { EXIT } from "@app/cli/exit.ts";
import { err, out, printError } from "@app/cli/runtime.ts";
import { buildContainer } from "@app/di/container.ts";
import { type ConfigValue, coerce } from "@features/inspect/cli/configFile.ts";
import { runDeaconConfig } from "@features/inspect/cli/deaconConfig.ts";
import { style } from "@shared/ui/style.ts";

export const APPLIES_NEXT_RUN =
  "(applies on the next `regent` command — the deacon reloads config each run)";

/** Ask a running deacon to make the change. null = it could not be reached. */
async function setViaRpc(profile: string, key: string, value: ConfigValue): Promise<number | null> {
  const deps = await buildContainer(profile);
  if (!deps.ok) return null;
  const { client } = deps.value;
  try {
    const health = await client.call("health", {}, 10_000);
    if (!health.ok) return null;
    const res = await client.call<{ note?: string }>("config.set", { path: key, value }, 30_000);
    if (!res.ok) {
      // A validation refusal is an answer, not an outage: report it and stop,
      // rather than retrying offline and getting the same refusal twice.
      printError(res.error.message);
      return EXIT.usage;
    }
    out(`set ${style.teal(key)} = ${style.value(JSON.stringify(value))}`);
    out(style.grey(res.value.note ?? APPLIES_NEXT_RUN));
    return EXIT.ok;
  } finally {
    await client.close();
  }
}

export async function configSetCommand(profile: string, args: string[]): Promise<number> {
  const bad = unknownFlags(args, {});
  if (bad.length > 0) {
    printError(`unknown option: ${bad.join(" ")}   (regent config set <key> <value>)`);
    return EXIT.usage;
  }
  const { positionals } = parseFlags(args, {});
  const [key, ...valueParts] = positionals;
  if (!key || valueParts.length === 0) {
    printError("usage: regent config set <key> <value>   (e.g. model.default claude-opus-4-8)");
    return EXIT.usage;
  }
  const value = coerce(valueParts.join(" "));

  // memory.home is informational only: the deacon picks its data directory
  // from REGENT_HOME (or -p <profile>) BEFORE config.yaml is read, so this
  // key can never take effect. Say so instead of silently no-opping.
  if (key === "memory.home") {
    printError(
      "memory.home has no effect — the data directory is chosen by the REGENT_HOME env var (or `regent -p <profile>`) before config is read. Set that instead and restart.",
    );
    return EXIT.usage;
  }

  const viaRpc = await setViaRpc(profile, key, value);
  if (viaRpc !== null) return viaRpc;

  // No daemon — same Rust code, run as a one-shot. Values go over as JSON so a
  // list stays a list and a number stays a number.
  const r = runDeaconConfig(profile, ["set", key, JSON.stringify(value)]);
  if (r.status === "ok") {
    out(`set ${style.teal(key)} = ${style.value(JSON.stringify(value))}`);
    out(style.grey(APPLIES_NEXT_RUN));
    return EXIT.ok;
  }
  if (r.status === "invalid") {
    printError(r.detail);
    return EXIT.usage;
  }
  if (r.status === "malformed") {
    printError(`config.yaml is not valid YAML and was left untouched: ${r.detail}`);
    err("  it has to be fixed by hand before any key can be set");
    return EXIT.failure;
  }
  printError(`cannot change config: ${r.detail}`);
  return EXIT.failure;
}
