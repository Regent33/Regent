// Setup's persistence: .env (secrets, owner-only atomic write) + config.yaml
// (behavior, through the deacon's validated write). Shared by the linear flag
// path and the Ink wizard so there is exactly one way setup writes to disk.
import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { type DeaconConfigResult, setConfigKeys } from "@features/inspect/cli/deaconConfig.ts";
import { lockDownFile } from "@shared/infrastructure/storage/lockdown.ts";

/** Upsert REGENT_API_KEY in .env, preserving other lines. Atomic temp→rename
 * at 0600; on Windows an owner-only ACL is applied after the rename. No key →
 * no write (the caller warns). */
export function writeEnv(home: string, key: string): void {
  if (!key) return;
  const path = join(home, ".env");
  const kept: string[] = [];
  try {
    for (const line of readFileSync(path, "utf8").split("\n")) {
      const t = line.trim();
      if (t === "" || t.startsWith("REGENT_API_KEY=")) continue;
      kept.push(line);
    }
  } catch {
    // no existing .env — fine
  }
  kept.push(`REGENT_API_KEY=${key}`);
  const tmp = join(home, `.env.tmp.${process.pid}`);
  writeFileSync(tmp, `${kept.join("\n")}\n`, { mode: 0o600 });
  renameSync(tmp, path);
  lockDownFile(path);
}

/**
 * Merge provider/model/base_url into config.yaml, preserving every other key
 * (re-running setup to switch provider must take effect). An empty base_url
 * removes the key so the deacon uses the provider's own default endpoint.
 *
 * This used to parse the file itself and, on a parse error, "start fresh" — i.e.
 * re-running setup on a machine with one bad line in config.yaml silently
 * replaced the whole file. The write now goes through the deacon's validated,
 * locked, atomic path like every other config change, so a malformed file is
 * REPORTED and left alone, and a provider the deacon does not know is refused
 * here instead of bricking the next launch.
 */
export function writeConfig(
  home: string,
  provider: string,
  model: string,
  baseUrl: string,
  constitution: boolean,
): DeaconConfigResult {
  mkdirSync(home, { recursive: true });
  return setConfigKeys(home, [
    ["model.provider", provider],
    ["model.default", model],
    // null, not "unset": it clears a stale override so the provider's own
    // endpoint is used, and reads back as an explicit choice rather than an
    // absence someone has to guess about.
    ["model.base_url", baseUrl || null],
    // The deacon seeds/clears the constitution persona row from this flag on boot.
    ["constitution.enabled", constitution],
  ]);
}
