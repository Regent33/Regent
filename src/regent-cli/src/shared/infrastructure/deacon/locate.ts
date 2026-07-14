// Resolves where the regent-deacon binary lives and what REGENT_HOME a profile
// maps to. Ported from the Go deacon.Locate so both front-ends agree on the
// search order: env override → sibling of this exe → PATH → cargo dev build.
import { existsSync, readFileSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { delimiter, dirname, join } from "node:path";
import { type Result, err, failure, ok } from "@shared/kernel/result.ts";

const EXE_SUFFIX = process.platform === "win32" ? ".exe" : "";

/** Resolve the regent-deacon binary. */
export function locateDeacon(): Result<string> {
  return locateBinary("regent-deacon", "REGENT_DEACON_PATH");
}

/** Resolve a Regent binary by base name (no extension). */
export function locateBinary(base: string, envVar: string): Result<string> {
  const override = process.env[envVar];
  if (override) {
    if (existsSync(override)) return ok(override);
    return err(failure("deacon-locate", `${envVar} set but not found: ${override}`));
  }

  const binaryName = base + EXE_SUFFIX;

  // Sibling of this executable (the compiled-binary install layout).
  const sibling = join(dirname(process.execPath), binaryName);
  if (existsSync(sibling)) return ok(sibling);

  // PATH lookup.
  for (const dir of (process.env.PATH ?? "").split(delimiter)) {
    if (dir && existsSync(join(dir, binaryName))) return ok(join(dir, binaryName));
  }

  // Cargo build: walk up from BOTH the CLI binary's location and the cwd looking
  // for target/{release,debug}. Walking up from the binary means `regent` finds
  // the deacon from any directory (dist/ → … → <repo>/target), not just when run
  // from inside the repo; the cwd walk covers `bun run dev` (binary = bun.exe).
  for (const start of [dirname(process.execPath), process.cwd()]) {
    const found = walkUpForTarget(start, binaryName);
    if (found) return ok(found);
  }

  return err(
    failure(
      "deacon-locate",
      `${base} not found (set ${envVar} or build with \`cargo build -p regent-deacon\`)`,
    ),
  );
}

// Walk up from `start` (max 8 levels) for target/release or target/debug.
function walkUpForTarget(start: string, binaryName: string): string | null {
  let dir = start;
  for (let i = 0; i < 8; i++) {
    const found = newestInTarget(dir, binaryName);
    if (found) return found;
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return null;
}

/** Newest of target/{release,debug}/<binaryName> under `dir` by mtime —
 * release-first order silently ran a stale release exe after a debug rebuild. */
export function newestInTarget(dir: string, binaryName: string): string | null {
  let best: { mtime: number; path: string } | null = null;
  for (const profile of ["release", "debug"]) {
    const candidate = join(dir, "target", profile, binaryName);
    if (!existsSync(candidate)) continue;
    const mtime = statSync(candidate).mtimeMs;
    if (!best || mtime > best.mtime) best = { mtime, path: candidate };
  }
  return best ? best.path : null;
}

/**
 * Map a profile name to its REGENT_HOME. Empty profile = $REGENT_HOME if set,
 * else the onboarding-chosen data dir (redirect file), else ~/.regent; a named
 * profile always isolates under ~/.regent-profiles (an explicit choice — env
 * never wins). The CLI passes this value to the spawned deacon's env, so one
 * resolution governs both processes.
 */
export function regentHome(profile: string): string {
  const base = homedir() || ".";
  if (profile) return join(base, ".regent-profiles", profile);
  if (process.env.REGENT_HOME) return process.env.REGENT_HOME;
  return redirectedHome(join(base, ".regent"));
}

/**
 * The onboarding wizard's "data directory" choice: `<default>/.home` holds one
 * absolute path. The pointer lives at the DEFAULT location because home is
 * resolved before any config can be read (config.yaml lives inside home).
 * Missing/empty/self-pointing file → the default. Exported for tests.
 */
export function redirectedHome(def: string): string {
  try {
    const target = readFileSync(join(def, ".home"), "utf8").trim();
    if (target !== "" && target !== def) return target;
  } catch {
    // no redirect file — the default home
  }
  return def;
}
