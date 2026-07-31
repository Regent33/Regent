// Offline config operations, run by the deacon binary as a short-lived process
// (`regent-deacon config …`) rather than over RPC.
//
// This is what let the CLI's own YAML writer be deleted. There is now exactly
// one implementation of "change a config key" — the Rust one that validates
// against the real `DeaconConfig`, locks, and replaces the file atomically — and
// the CLI reaches it two ways: over RPC when a deacon is running (so open
// sessions pick the change up live), and by spawning the binary when one is not.
import { deaconCandidates, regentHome } from "@shared/infrastructure/deacon/locate.ts";

export interface DeaconConfigResult {
  /** "ok" | "invalid" | "malformed" | "not_set", or null when nothing ran. */
  readonly status: string | null;
  readonly detail: string;
  readonly json: Record<string, unknown> | null;
}

const NO_BINARY: DeaconConfigResult = {
  status: null,
  detail: "no regent-deacon binary found (set REGENT_DEACON_PATH or build it)",
  json: null,
};

/**
 * Run one offline config op. Tries every candidate binary, because a stale
 * pinned path is a normal state on a half-upgraded install and the next
 * candidate usually works.
 */
export function runDeaconConfig(profile: string, args: string[]): DeaconConfigResult {
  const candidates = deaconCandidates();
  if (candidates.length === 0) return NO_BINARY;
  let last = NO_BINARY;
  for (const exe of candidates) {
    const r = Bun.spawnSync([exe, "config", ...args], {
      env: { ...process.env, REGENT_HOME: regentHome(profile) },
      stdin: new Uint8Array(),
    });
    const stdout = r.stdout.toString().trim();
    if (stdout === "") {
      // Did not even reach the subcommand — an old deacon without it, or a
      // binary that cannot start. Try the next candidate.
      last = { status: null, detail: r.stderr.toString().trim() || "no output", json: null };
      continue;
    }
    try {
      const json = JSON.parse(stdout) as Record<string, unknown>;
      return {
        status: typeof json.status === "string" ? json.status : "ok",
        detail: typeof json.detail === "string" ? json.detail : "",
        json,
      };
    } catch {
      last = {
        status: null,
        detail: `unreadable deacon output: ${stdout.slice(0, 200)}`,
        json: null,
      };
    }
  }
  return last;
}
