// Offline config.yaml handling. Everything here works with no deacon, because
// a broken config is exactly the state in which the deacon will not start.
//
// It deliberately does NOT know the config schema: the Rust `DeaconConfig` owns
// that, and a second copy in TypeScript would drift. What it owns is the file:
// parse it or refuse, hold a lock, write atomically, keep the permissions.
import {
  chmodSync,
  closeSync,
  existsSync,
  fsyncSync,
  mkdirSync,
  openSync,
  readFileSync,
  renameSync,
  statSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";
import YAML from "yaml";

export type ConfigDoc = Record<string, unknown>;

export type ReadResult =
  | { kind: "ok"; doc: ConfigDoc; raw: string }
  | { kind: "missing" }
  /** The file exists but is not YAML. Nothing here may rewrite it. */
  | { kind: "malformed"; detail: string };

export function configPath(home: string): string {
  return join(home, "config.yaml");
}

/**
 * Read config.yaml, keeping "not there" and "not parseable" apart. The old
 * `config set` collapsed both into "start fresh" and wrote a brand-new file
 * over the unreadable one — silent, total loss of the user's config.
 */
export function readConfig(home: string): ReadResult {
  const path = configPath(home);
  if (!existsSync(path)) return { kind: "missing" };
  let raw: string;
  try {
    raw = readFileSync(path, "utf8");
  } catch (e) {
    return { kind: "malformed", detail: e instanceof Error ? e.message : String(e) };
  }
  try {
    const parsed = YAML.parse(raw) as unknown;
    if (parsed === null || parsed === undefined) return { kind: "ok", doc: {}, raw };
    if (typeof parsed !== "object" || Array.isArray(parsed)) {
      return { kind: "malformed", detail: "top level is not a mapping" };
    }
    return { kind: "ok", doc: parsed as ConfigDoc, raw };
  } catch (e) {
    return { kind: "malformed", detail: e instanceof Error ? e.message : String(e) };
  }
}

/**
 * Hold an exclusive lock for the duration of a read-modify-write. `.bak` files
 * and atomic renames do not prevent a lost update: two `config set` processes
 * can both read, both edit their copy, and the second rename wins.
 *
 * ponytail: a crashed writer leaves the lock behind and the message says to
 * delete it. Stealing it on an age heuristic is the upgrade path if that ever
 * actually bites someone.
 */
export function withConfigLock<T>(home: string, fn: () => T, waitMs = 5_000): T {
  // The home may not exist yet on a first `config set`. Without this, `wx`
  // fails with ENOENT and the retry loop misreports it as contention.
  mkdirSync(home, { recursive: true });
  const lock = `${configPath(home)}.lock`;
  const token = `${process.pid}\n`;
  const deadline = Date.now() + waitMs;
  let fd: number | undefined;
  for (;;) {
    try {
      fd = openSync(lock, "wx");
      break;
    } catch (e) {
      // Only "it already exists" is contention. A permissions or path error is
      // a real failure and must not be retried for five seconds and relabelled.
      if ((e as NodeJS.ErrnoException).code !== "EEXIST") throw e;
      if (Date.now() >= deadline) {
        throw new Error(
          `another regent process is writing config.yaml (lock held). If none is running, delete ${lock}`,
        );
      }
      Bun.sleepSync(50);
    }
  }
  writeFileSync(fd, token);
  try {
    return fn();
  } finally {
    closeSync(fd);
    try {
      // Only release our own lock: if someone deleted it mid-hold (the timeout
      // message tells them to) another writer may already own the replacement.
      if (readFileSync(lock, "utf8") === token) unlinkSync(lock);
    } catch {
      // Already gone — nothing to release.
    }
  }
}

/**
 * Write via a same-directory temp file and rename. The temp file is created
 * private and only then widened to the existing file's mode, so the contents
 * are never briefly exposed under a loose umask, and it is flushed before the
 * rename so a crash cannot leave a zero-length config behind.
 */
export function writeConfigAtomically(home: string, doc: ConfigDoc): void {
  const path = configPath(home);
  const tmp = `${path}.tmp.${process.pid}`;
  const fd = openSync(tmp, "w", 0o600);
  try {
    writeFileSync(fd, YAML.stringify(doc));
    fsyncSync(fd);
  } finally {
    closeSync(fd);
  }
  // A config the user tightened to 0600 must not come back world-readable, and
  // a failure here leaves 0600 — restrictive, never looser than intended.
  try {
    if (existsSync(path)) chmodSync(tmp, statSync(path).mode & 0o777);
  } catch {
    // Filesystem without POSIX modes (or a racing delete): keep the write.
  }
  renameSync(tmp, path);
}

// Segments that would walk off the document into JavaScript's own object graph.
const FORBIDDEN = new Set(["__proto__", "prototype", "constructor"]);

/** Split and check a dotted path. Throws on anything that is not a config key. */
function segments(dotted: string): string[] {
  const parts = dotted.split(".");
  for (const p of parts) {
    if (p === "") throw new Error(`invalid key "${dotted}": empty path segment`);
    if (FORBIDDEN.has(p)) throw new Error(`invalid key "${dotted}": "${p}" is not a config key`);
  }
  return parts;
}

/** Set a dotted key, creating intermediate mappings. */
export function setDotted(root: ConfigDoc, dotted: string, value: unknown): void {
  const keys = segments(dotted);
  let node = root;
  for (const k of keys.slice(0, -1)) {
    if (typeof node[k] !== "object" || node[k] === null || Array.isArray(node[k])) node[k] = {};
    node = node[k] as ConfigDoc;
  }
  node[keys[keys.length - 1] as string] = value;
}

/** Remove a dotted key. Returns false when it was not there to begin with. */
export function unsetDotted(root: ConfigDoc, dotted: string): boolean {
  const keys = segments(dotted);
  let node = root;
  for (const k of keys.slice(0, -1)) {
    const next = node[k];
    if (typeof next !== "object" || next === null || Array.isArray(next)) return false;
    node = next as ConfigDoc;
  }
  const last = keys[keys.length - 1] as string;
  if (!Object.hasOwn(node, last)) return false;
  delete node[last];
  return true;
}

/** What a coerced CLI argument can become on its way into config.yaml. */
export type ConfigValue = string | number | boolean | Array<string | number | boolean>;

/** Coerce a CLI string to a YAML value: booleans, plain numbers and JSON arrays
 * get typed; everything else stays a string.
 *
 * Arrays are here because several config keys are lists — `tools.deferred`,
 * `tools.pinned`, `providers.<name>.models`. Without this, `config set
 * tools.deferred '["a","b"]'` stored the BRACKETS AS TEXT: config.yaml came out
 * holding `deferred: '["a","b"]'`, a single string, and tool deferral silently
 * stopped matching anything. It was worse than a visible failure because the
 * write reported success. */
export function coerce(value: string): ConfigValue {
  if (value === "true") return true;
  if (value === "false") return false;
  if (/^-?\d+(\.\d+)?$/.test(value)) return Number(value);
  // Only a well-formed JSON array of scalars. A malformed one falls through to
  // string rather than throwing, so a value that merely LOOKS bracketed (a
  // prompt fragment, a glob) is still settable.
  if (value.startsWith("[") && value.endsWith("]")) {
    try {
      const parsed: unknown = JSON.parse(value);
      if (Array.isArray(parsed) && parsed.every((item) => item !== null && typeof item !== "object")) {
        return parsed as Array<string | number | boolean>;
      }
    } catch {
      // not JSON — fall through and store it verbatim
    }
  }
  return value;
}
