// Setup's persistence: .env (secrets, owner-only atomic write) + config.yaml
// (behavior, merge-preserving). Shared by the linear flag path and the Ink
// wizard so there is exactly one way setup writes to disk.
import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { lockDownFile } from "@shared/infrastructure/storage/lockdown.ts";
import YAML from "yaml";

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

/** Merge provider/model/base_url into config.yaml, preserving every other key
 * (re-running setup to switch provider must take effect). An empty base_url
 * removes the key so the deacon uses the provider's own default endpoint. */
export function writeConfig(
  home: string,
  provider: string,
  model: string,
  baseUrl: string,
  constitution: boolean,
): void {
  const path = join(home, "config.yaml");
  let doc: Record<string, unknown> = {};
  try {
    const parsed = YAML.parse(readFileSync(path, "utf8")) as unknown;
    if (parsed && typeof parsed === "object") doc = parsed as Record<string, unknown>;
  } catch {
    // no / invalid config.yaml — start fresh
  }
  doc._config_version = doc._config_version ?? 1;
  const m = (typeof doc.model === "object" && doc.model !== null ? doc.model : {}) as Record<
    string,
    unknown
  >;
  m.provider = provider;
  m.default = model;
  // undefined keys are omitted by YAML.stringify — clears a stale override.
  m.base_url = baseUrl ? baseUrl : undefined;
  doc.model = m;
  // The deacon seeds/clears the constitution persona row from this flag on boot.
  doc.constitution = { enabled: constitution };

  mkdirSync(home, { recursive: true });
  const tmp = join(home, `config.yaml.tmp.${process.pid}`);
  writeFileSync(tmp, YAML.stringify(doc), { mode: 0o644 });
  renameSync(tmp, path);
}
