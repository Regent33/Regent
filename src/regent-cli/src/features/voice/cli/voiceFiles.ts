// Config/.env persistence for `regent voice` — atomic writes that preserve
// other keys. The daemon reads config.yaml; the gateway reads .env.
import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import YAML from "yaml";

export function readConfig(home: string): Record<string, unknown> {
  try {
    const parsed = YAML.parse(readFileSync(join(home, "config.yaml"), "utf8")) as unknown;
    if (parsed && typeof parsed === "object") return parsed as Record<string, unknown>;
  } catch {
    // no / invalid config.yaml — start fresh
  }
  return {};
}

export function writeConfig(home: string, doc: Record<string, unknown>): void {
  if (doc._config_version === undefined) doc._config_version = 1;
  mkdirSync(home, { recursive: true });
  const tmp = join(home, `config.yaml.tmp.${process.pid}`);
  writeFileSync(tmp, YAML.stringify(doc));
  renameSync(tmp, join(home, "config.yaml"));
}

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
