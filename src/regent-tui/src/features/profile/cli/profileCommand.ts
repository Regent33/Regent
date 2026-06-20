// `regent profile list|create|delete` — manage profile homes under
// ~/.regent-profiles/. Pure filesystem; no daemon. `regent -p <name> <cmd>`
// then runs any command against that profile's REGENT_HOME.
import { existsSync, mkdirSync, readdirSync, rmSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { parseFlags } from "@app/cli/args.ts";
import { out, printError } from "@app/cli/runtime.ts";
import { style } from "@shared/ui/style.ts";

const profilesDir = (): string => join(homedir() || ".", ".regent-profiles");

export function profileCommand(args: string[]): number {
  const { positionals, values } = parseFlags(args, { force: { type: "boolean" } });
  const [sub, name] = positionals;
  const dir = profilesDir();

  if (sub === "create") {
    if (!name) {
      printError("usage: regent profile create <name>");
      return 1;
    }
    const path = join(dir, name);
    if (existsSync(path)) {
      out(style.grey(`profile '${name}' already exists`));
      return 0;
    }
    mkdirSync(path, { recursive: true, mode: 0o700 });
    out(`created profile ${style.teal(name)} (${path})`);
    return 0;
  }

  if (sub === "delete") {
    if (!name) {
      printError("usage: regent profile delete <name> --force");
      return 1;
    }
    const path = join(dir, name);
    if (!existsSync(path)) {
      printError(`no profile '${name}'`);
      return 1;
    }
    // Destructive: a profile home holds state.db + .env secrets. Require --force.
    if (!values.force) {
      printError(
        `deleting '${name}' removes its state.db and .env — re-run with --force to confirm`,
      );
      return 1;
    }
    rmSync(path, { recursive: true, force: true });
    out(`deleted profile ${style.teal(name)}`);
    return 0;
  }

  // Default: list.
  if (!existsSync(dir)) {
    out(style.grey("no profiles yet — create one with `regent profile create <name>`"));
    return 0;
  }
  const names = readdirSync(dir).filter((n) => {
    try {
      return statSync(join(dir, n)).isDirectory();
    } catch {
      return false;
    }
  });
  if (names.length === 0) {
    out(style.grey("no profiles yet"));
    return 0;
  }
  for (const n of names) out(style.teal(n));
  return 0;
}
