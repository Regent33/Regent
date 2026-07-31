// .env persistence for `regent voice` — an atomic write that preserves other
// keys. The gateway reads .env; config.yaml goes through the deacon's validated
// write (deaconConfig.setConfigKeys), not from here.
import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";

/** Upsert `KEY=value` lines in `$home/.env`, preserving the rest. Mode 0600. */
export function upsertEnv(home: string, updates: Record<string, string>): void {
  const path = join(home, ".env");
  const kept: string[] = [];
  try {
    for (const line of readFileSync(path, "utf8").split("\n")) {
      const key = line.slice(0, Math.max(0, line.indexOf("="))).trim();
      if (line.trim() === "" || key in updates) continue;
      kept.push(line);
    }
  } catch {
    // no existing .env
  }
  for (const [k, v] of Object.entries(updates)) kept.push(`${k}=${v}`);
  mkdirSync(home, { recursive: true });
  const tmp = join(home, `.env.tmp.${process.pid}`);
  writeFileSync(tmp, `${kept.join("\n")}\n`, { mode: 0o600 });
  renameSync(tmp, path);
}
