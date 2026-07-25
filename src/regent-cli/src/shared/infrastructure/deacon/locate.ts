// Resolves where the regent-deacon binary lives and what REGENT_HOME a profile
// maps to. Ported from the Go deacon.Locate so both front-ends agree on the
// search order: env override → sibling → newest cargo build → PATH.
import { existsSync, readFileSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { delimiter, dirname, join, resolve } from "node:path";
import { err, failure, ok, type Result } from "@shared/kernel/result.ts";

const EXE_SUFFIX = process.platform === "win32" ? ".exe" : "";

/** Resolve the regent-deacon binary (the first candidate; see `deaconCandidates`). */
export function locateDeacon(): Result<string> {
  return locateBinary("regent-deacon", "REGENT_DEACON_PATH");
}

/** Resolve a Regent binary by base name (no extension) — the first candidate. */
export function locateBinary(base: string, envVar: string): Result<string> {
  const override = process.env[envVar];
  // An override that points nowhere is a config error worth surfacing directly,
  // matching the historical single-pick behavior gateway/mcp callers rely on.
  if (override && !existsSync(override)) {
    return err(failure("deacon-locate", `${envVar} set but not found: ${override}`));
  }
  const [first] = binaryCandidates(base, envVar);
  if (first) return ok(first);
  return err(
    failure(
      "deacon-locate",
      `${base} not found (set ${envVar} or build with \`cargo build -p regent-deacon\`)`,
    ),
  );
}

/** Ordered, de-duplicated regent-deacon candidates to spawn/health-probe in turn. */
export function deaconCandidates(): string[] {
  return binaryCandidates("regent-deacon", "REGENT_DEACON_PATH");
}

/**
 * Every place a `base` binary might live, most-preferred first and de-duplicated
 * (case-insensitively on Windows), keeping only paths that exist on disk:
 *   1. the `envVar` override — kept FIRST in order, but no longer decisive: a
 *      pinned binary that boots then dies (e.g. an old REGENT_DEACON_PATH whose
 *      config schema drifted) is spawned first, fails its health probe, and the
 *      caller falls through instead of surfacing a bare EPIPE;
 *   2. this executable's sibling (the compiled-install layout);
 *   3. the NEWEST target/{release,debug} walked up from BOTH this exe and the
 *      cwd — placed BEFORE PATH so a fresh repo build beats a stale install;
 *   4. PATH entries.
 * The cwd walk covers `bun run dev` (execPath = bun.exe); the exe walk lets an
 * installed `regent` find the deacon from any directory.
 */
export function binaryCandidates(base: string, envVar: string): string[] {
  const binaryName = base + EXE_SUFFIX;
  const out: string[] = [];
  const seen = new Set<string>();
  const add = (candidate: string | null | undefined): void => {
    if (!candidate || !existsSync(candidate)) return;
    const abs = resolve(candidate);
    const key = process.platform === "win32" ? abs.toLowerCase() : abs;
    if (seen.has(key)) return;
    seen.add(key);
    out.push(abs);
  };

  add(process.env[envVar]);
  add(join(dirname(process.execPath), binaryName));
  for (const start of [dirname(process.execPath), process.cwd()]) {
    add(walkUpForTarget(start, binaryName));
  }
  for (const dir of (process.env.PATH ?? "").split(delimiter)) {
    if (dir) add(join(dir, binaryName));
  }
  return out;
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
