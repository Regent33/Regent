// `regent config unset|validate|list` — the offline half of the config surface.
// They exist for the state in which the deacon is not running: a config.yaml it
// refuses to load, so the RPC path is unavailable by definition.
//
// None of them re-implement anything. Each one runs `regent-deacon config …`,
// which validates against the real `DeaconConfig`. That is why `validate` can
// say "valid" and mean it, instead of "the YAML parsed, good luck".
import { EXIT } from "@app/cli/exit.ts";
import { err, out, printError } from "@app/cli/runtime.ts";
import { runDeaconConfig } from "@features/inspect/cli/deaconConfig.ts";
import { style } from "@shared/ui/style.ts";

const APPLIES_NEXT_RUN =
  "(applies on the next `regent` command — the deacon reloads config each run)";

/** Turn a non-ok status into the right message and exit code. */
function report(status: string | null, detail: string, key?: string): number {
  if (status === "malformed") {
    printError(`config.yaml is not valid YAML and was left untouched: ${detail}`);
    err("  it has to be fixed by hand — `config unset` cannot parse it either");
    return EXIT.failure;
  }
  if (status === "invalid") {
    printError(detail);
    return EXIT.failure;
  }
  if (status === "not_set") {
    printError(key ? `${key} is not set in config.yaml` : detail);
    return EXIT.failure;
  }
  printError(`cannot run the config check: ${detail}`);
  return EXIT.failure;
}

/** `regent config unset <key>` — offline repair for a schema-invalid config. */
export function configUnsetCommand(profile: string, args: string[]): number {
  const key = args[0];
  if (!key) {
    printError("usage: regent config unset <key>   (e.g. regent config unset model.defalut)");
    return EXIT.usage;
  }
  const r = runDeaconConfig(profile, ["unset", key]);
  if (r.status !== "ok") return report(r.status, r.detail, key);
  out(`unset ${style.teal(key)}`);
  out(style.grey(APPLIES_NEXT_RUN));
  return EXIT.ok;
}

/** `regent config validate` — a real schema check, with no daemon running. */
export function configValidateCommand(profile: string): number {
  const r = runDeaconConfig(profile, ["validate"]);
  if (r.status === "ok") {
    out(`${style.pass("✓")} config.yaml loads and validates`);
    return EXIT.ok;
  }
  if (r.status === "invalid") {
    printError(`config.yaml did not validate: ${r.detail}`);
    err("  `regent config unset <key>` removes an offending key");
    return EXIT.failure;
  }
  return report(r.status, r.detail);
}

/** `regent config list` — every key the config type defines, with its value. */
export function configListCommand(profile: string, args: string[]): number {
  const r = runDeaconConfig(profile, ["describe"]);
  if (r.json === null) return report(r.status, r.detail);
  const keys = (r.json.keys ?? []) as Array<Record<string, unknown>>;
  if (args.includes("--json")) {
    out(JSON.stringify(r.json, null, 2));
    return EXIT.ok;
  }
  // The descriptor falls back to defaults when the file will not parse. Listing
  // those as if they were the answer told someone whose config.yaml was full of
  // settings that "every key is at its default" — the opposite of the truth.
  if (r.json.file === "malformed") {
    printError(`config.yaml could not be read: ${String(r.json.file_detail ?? "")}`);
    err("  nothing is listed because the only values available are the defaults,");
    err("  and printing those would read as 'you have changed nothing'");
    err("  `regent config list --all --json` still shows what CAN be set");
    return EXIT.failure;
  }
  // Only the keys a user actually changed, unless they ask for everything —
  // fifty-odd defaults is a wall, and "what did I set" is the real question.
  const all = args.includes("--all");
  const shown = all ? keys : keys.filter((k) => k.origin === "config.yaml");
  if (shown.length === 0) {
    out(style.grey("every key is at its default — `regent config list --all` shows them"));
    return EXIT.ok;
  }
  const width = Math.max(...shown.map((k) => String(k.path).length));
  for (const k of shown) {
    // Secrets arrive already redacted: the deacon reports presence, never value.
    const value = style.value(JSON.stringify(k.value));
    const origin = k.origin === "config.yaml" ? "" : style.grey("  (default)");
    out(`  ${String(k.path).padEnd(width)}  ${value}${origin}`);
  }
  if (!all) out(style.grey(`\n${keys.length - shown.length} more at their defaults — --all`));
  return EXIT.ok;
}
