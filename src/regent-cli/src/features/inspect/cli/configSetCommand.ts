// `regent config set|unset|validate` — the config-editing surface.
//
// `set` prefers the deacon's `config.set` RPC, which proves the WHOLE file still
// deserialises into the real `DeaconConfig` (deny_unknown_fields + the provider
// enum) before it writes. The CLI used to hand-edit the YAML instead, so a typo
// like `moddel.default` was written happily and bricked the next launch.
//
// The offline path stays, because a config bad enough to stop the deacon is
// exactly when you need to edit it — but it says plainly that it could not
// validate, and it never rewrites a file it failed to parse.
import { parseFlags, unknownFlags } from "@app/cli/args.ts";
import { EXIT } from "@app/cli/exit.ts";
import { err, out, printError } from "@app/cli/runtime.ts";
import { buildContainer } from "@app/di/container.ts";
import {
  type ConfigValue,
  coerce,
  configPath,
  readConfig,
  setDotted,
  withConfigLock,
  writeConfigAtomically,
} from "@features/inspect/cli/configFile.ts";
import { regentHome } from "@shared/infrastructure/deacon/locate.ts";
import { style } from "@shared/ui/style.ts";

export const APPLIES_NEXT_RUN =
  "(applies on the next `regent` command — the deacon reloads config each run)";

/** Ask the deacon to make the change. null = the deacon could not be reached. */
async function setViaDeacon(
  profile: string,
  key: string,
  value: ConfigValue,
): Promise<number | null> {
  const deps = await buildContainer(profile);
  if (!deps.ok) return null;
  const { client } = deps.value;
  try {
    const health = await client.call("health", {}, 10_000);
    if (!health.ok) return null;
    const res = await client.call<{ note?: string }>("config.set", { path: key, value }, 30_000);
    if (!res.ok) {
      // A validation refusal is an answer, not an outage: report it and stop
      // rather than falling back to the unvalidated writer and undoing the gate.
      printError(res.error.message);
      return EXIT.usage;
    }
    out(`set ${style.teal(key)} = ${style.value(String(value))}`);
    out(style.grey(res.value.note ?? APPLIES_NEXT_RUN));
    return EXIT.ok;
  } finally {
    await client.close();
  }
}

export function malformedError(home: string, detail: string): number {
  printError(`config.yaml is not valid YAML and was left untouched: ${detail}`);
  err(`  fix it by hand: ${configPath(home)}`);
  return EXIT.failure;
}

export async function configSetCommand(profile: string, args: string[]): Promise<number> {
  const spec = { offline: { type: "boolean" } } as const;
  const bad = unknownFlags(args, spec);
  if (bad.length > 0) {
    printError(`unknown option: ${bad.join(" ")}   (regent config set <key> <value> [--offline])`);
    return EXIT.usage;
  }
  const { values, positionals } = parseFlags(args, spec);
  const [key, ...valueParts] = positionals;
  if (!key || valueParts.length === 0) {
    printError("usage: regent config set <key> <value>   (e.g. model.default claude-opus-4-8)");
    return EXIT.usage;
  }
  const value = valueParts.join(" ");
  const home = regentHome(profile);

  // memory.home is informational only: the deacon picks its data directory
  // from REGENT_HOME (or -p <profile>) BEFORE config.yaml is read, so this
  // key can never take effect. Say so instead of silently no-opping.
  if (key === "memory.home") {
    printError(
      "memory.home has no effect — the data directory is chosen by the REGENT_HOME env var (or `regent -p <profile>`) before config is read. Set that instead and restart.",
    );
    return EXIT.usage;
  }

  const viaDeacon = await setViaDeacon(profile, key, coerce(value));
  if (viaDeacon !== null) return viaDeacon;

  // The deacon is the only thing that can prove a write is safe. Without it,
  // writing anyway is how a config gets bricked, so it takes an explicit ask.
  if (values.offline !== true) {
    printError("cannot reach the deacon, so this change cannot be validated before writing.");
    err("  `regent config unset <key>` repairs a bad key without needing the deacon.");
    err("  `regent doctor` says why the deacon will not start.");
    err("  To write it anyway, unvalidated: regent config set <key> <value> --offline");
    return EXIT.failure;
  }

  let failure: string | null = null;
  try {
    withConfigLock(home, () => {
      // Read INSIDE the lock: reading first and locking second still loses an
      // update when two writers interleave.
      const current = readConfig(home);
      if (current.kind === "malformed") {
        failure = current.detail;
        return;
      }
      const doc = current.kind === "ok" ? current.doc : {};
      setDotted(doc, key, coerce(value));
      writeConfigAtomically(home, doc);
    });
  } catch (e) {
    printError(e instanceof Error ? e.message : String(e));
    return EXIT.failure;
  }
  if (failure !== null) return malformedError(home, failure);
  out(`set ${style.teal(key)} = ${style.value(value)}`);
  err(
    style.warn("! written WITHOUT validation (--offline) — a bad key or value will only surface"),
  );
  err(style.warn("  when the deacon next starts. Verify with `regent doctor`."));
  return EXIT.ok;
}
