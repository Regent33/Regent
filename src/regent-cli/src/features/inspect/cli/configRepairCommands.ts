// `regent config unset|validate` — the offline repair pair. They exist for the
// state in which `config set` cannot help: a config.yaml the deacon refuses to
// load, so the deacon-validated write path is unavailable by definition.
import { EXIT } from "@app/cli/exit.ts";
import { err, out, printError } from "@app/cli/runtime.ts";
import { buildContainer } from "@app/di/container.ts";
import {
  configPath,
  readConfig,
  unsetDotted,
  withConfigLock,
  writeConfigAtomically,
} from "@features/inspect/cli/configFile.ts";
import { APPLIES_NEXT_RUN, malformedError } from "@features/inspect/cli/configSetCommand.ts";
import { regentHome } from "@shared/infrastructure/deacon/locate.ts";
import { style } from "@shared/ui/style.ts";

/** `regent config unset <key>` — offline repair for a schema-invalid config. */
export function configUnsetCommand(profile: string, args: string[]): number {
  const key = args[0];
  if (!key) {
    printError("usage: regent config unset <key>   (e.g. regent config unset model.defalut)");
    return EXIT.usage;
  }
  const home = regentHome(profile);
  // Deliberately offline and deliberately not deacon-gated: the reason to reach
  // for `unset` is a key the deacon refuses to load. Removing a key can only
  // move the file towards validity, which is why this one needs no --offline.
  let removed = false;
  let missing = false;
  let malformed: string | null = null;
  try {
    withConfigLock(home, () => {
      const current = readConfig(home);
      if (current.kind === "missing") {
        missing = true;
        return;
      }
      if (current.kind === "malformed") {
        malformed = current.detail;
        return;
      }
      removed = unsetDotted(current.doc, key);
      if (removed) writeConfigAtomically(home, current.doc);
    });
  } catch (e) {
    printError(e instanceof Error ? e.message : String(e));
    return EXIT.failure;
  }
  if (missing) {
    printError("no config.yaml — nothing to unset");
    return EXIT.failure;
  }
  if (malformed !== null) return malformedError(home, malformed);
  if (!removed) {
    printError(`${key} is not set in config.yaml`);
    return EXIT.failure;
  }
  out(`unset ${style.teal(key)}`);
  out(style.grey(APPLIES_NEXT_RUN));
  return EXIT.ok;
}

/**
 * `regent config validate`. YAML well-formedness is checked here; the SCHEMA is
 * checked by starting the deacon, because the deacon's `DeaconConfig` is the
 * only definition of a valid config and a second one in TypeScript would drift.
 *
 * It never claims success it did not verify: an unreachable deacon means the
 * schema is unchecked, and unchecked is reported as a failure to validate — a
 * `validate` that exits 0 without validating is worse than no command at all.
 */
export async function configValidateCommand(profile: string): Promise<number> {
  const home = regentHome(profile);
  const current = readConfig(home);
  if (current.kind === "missing") {
    out(`${style.pass("✓")} no config.yaml — the deacon will create one from defaults`);
    return EXIT.ok;
  }
  if (current.kind === "malformed") {
    printError(`config.yaml is not valid YAML: ${current.detail}`);
    err(`  ${configPath(home)}`);
    return EXIT.failure;
  }

  const deps = await buildContainer(profile);
  if (deps.ok) {
    const { client } = deps.value;
    try {
      // A deacon that answers has already loaded and deserialised config.yaml.
      const health = await client.call("health", {}, 10_000);
      if (health.ok) {
        out(`${style.pass("✓")} config.yaml loads and validates`);
        return EXIT.ok;
      }
      printError(`config.yaml did not validate: ${health.error.message}`);
      err("  `regent config unset <key>` removes an offending key.");
      return EXIT.failure;
    } finally {
      await client.close();
    }
  }
  // The deacon deserialises config.yaml at startup, so "it would not start" is
  // usually the schema error itself — and the reason is in the message either way.
  printError("config.yaml is valid YAML, but the deacon rejected it or could not start:");
  err(`  ${deps.error.message}`);
  err("  `regent config unset <key>` removes an offending key · `regent doctor` for the rest");
  return EXIT.failure;
}
