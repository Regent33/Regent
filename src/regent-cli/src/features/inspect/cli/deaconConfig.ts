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

/** Same, for callers that already resolved REGENT_HOME (setup, voice). */
export function runDeaconConfig(profile: string, args: string[]): DeaconConfigResult {
  return runDeaconConfigIn(regentHome(profile), args);
}

/**
 * Run one offline config op. Tries every candidate binary, because a stale
 * pinned path is a normal state on a half-upgraded install and the next
 * candidate usually works.
 */
export function runDeaconConfigIn(home: string, args: string[]): DeaconConfigResult {
  const candidates = deaconCandidates();
  if (candidates.length === 0) return NO_BINARY;
  let last = NO_BINARY;
  for (const exe of candidates) {
    const r = Bun.spawnSync([exe, "config", ...args], {
      env: { ...process.env, REGENT_HOME: home },
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

/**
 * Apply several keys in ONE validated, locked transaction — all of them or none.
 *
 * Two rules worth stating, because getting either wrong is a data-loss bug:
 *   * address LEAF keys, not sections. Writing `speech` or `model` whole
 *     replaces every key under it with this command's idea of the section, so a
 *     sibling nobody here knows about (`model.review`, a hand-listed
 *     `speech.asr.weights`) disappears.
 *   * `null` is a real value — it clears an Option, e.g. `model.base_url` back
 *     to the provider's own endpoint. Removing a key entirely is `unset`.
 */
export function setConfigKeys(
  home: string,
  entries: readonly (readonly [string, unknown])[],
): DeaconConfigResult {
  if (entries.length === 0) return { status: "ok", detail: "", json: null };
  const args = entries.flatMap(([key, value]) => [key, JSON.stringify(value)]);
  return runDeaconConfigIn(home, ["set", ...args]);
}

/**
 * The EFFECTIVE value of one dotted key, from the deacon's own descriptor —
 * defaults included, so a key absent from config.yaml still reads as what the
 * daemon will use. `undefined` means the read failed and the caller must stop.
 *
 * The `file` check is load-bearing, not defensive. `describe` deliberately
 * falls back to defaults on an unparseable config so a broken install can still
 * answer "what can I set?" — which makes the descriptor alone unsafe as the
 * READ half of a read-modify-write: it would report an empty list for a config
 * full of settings, and the write would put that emptiness back.
 */
export function readConfigKey(home: string, key: string): unknown {
  const r = runDeaconConfigIn(home, ["describe"]);
  if (r.json === null || r.json.file === "malformed") return undefined;
  const keys = (r.json.keys ?? []) as Array<Record<string, unknown>>;
  return keys.find((k) => k.path === key)?.value;
}

/** One line explaining a refusal, in the terms of the file rather than the op. */
export function explainConfigFailure(r: DeaconConfigResult): string {
  if (r.status === "malformed") {
    return `config.yaml is not valid YAML and was left untouched: ${r.detail}\n  it has to be fixed by hand first`;
  }
  if (r.status === "invalid") return `rejected: ${r.detail}`;
  return `cannot change config: ${r.detail}`;
}
