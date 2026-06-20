// Resolves where the regent-daemon binary lives and what REGENT_HOME a profile
// maps to. Ported from the Go daemon.Locate so both front-ends agree on the
// search order: env override → sibling of this exe → PATH → cargo dev build.
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { delimiter, dirname, join } from "node:path";
import { type Result, err, failure, ok } from "@shared/kernel/result.ts";

const EXE_SUFFIX = process.platform === "win32" ? ".exe" : "";

/** Resolve the regent-daemon binary. */
export function locateDaemon(): Result<string> {
  return locateBinary("regent-daemon", "REGENT_DAEMON_PATH");
}

/** Resolve a Regent binary by base name (no extension). */
export function locateBinary(base: string, envVar: string): Result<string> {
  const override = process.env[envVar];
  if (override) {
    if (existsSync(override)) return ok(override);
    return err(failure("daemon-locate", `${envVar} set but not found: ${override}`));
  }

  const binaryName = base + EXE_SUFFIX;

  // Sibling of this executable (the compiled-binary install layout).
  const sibling = join(dirname(process.execPath), binaryName);
  if (existsSync(sibling)) return ok(sibling);

  // PATH lookup.
  for (const dir of (process.env.PATH ?? "").split(delimiter)) {
    if (dir && existsSync(join(dir, binaryName))) return ok(join(dir, binaryName));
  }

  // Dev fallback: walk up from cwd looking for the cargo target dir.
  let dir = process.cwd();
  for (let i = 0; i < 6; i++) {
    const candidate = join(dir, "target", "debug", binaryName);
    if (existsSync(candidate)) return ok(candidate);
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }

  return err(
    failure(
      "daemon-locate",
      `${base} not found (set ${envVar} or build with \`cargo build -p regent-daemon\`)`,
    ),
  );
}

/**
 * Map a profile name to its REGENT_HOME. Empty profile = $REGENT_HOME if set,
 * else ~/.regent; a named profile always isolates under ~/.regent-profiles
 * (an explicit choice — env never wins).
 */
export function regentHome(profile: string): string {
  const base = homedir() || ".";
  if (!profile) {
    return process.env.REGENT_HOME || join(base, ".regent");
  }
  return join(base, ".regent-profiles", profile);
}
