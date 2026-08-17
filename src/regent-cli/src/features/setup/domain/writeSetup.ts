// Setup's persistence: .env (secrets, owner-only atomic write) + config.yaml
// (behavior, through the deacon's validated write). Shared by the linear flag
// path and the Ink wizard so there is exactly one way setup writes to disk.
import { mkdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { type DeaconConfigResult, setConfigKeys } from "@features/inspect/cli/deaconConfig.ts";
import { updateDotenv } from "@shared/infrastructure/storage/dotenvFile.ts";
import { lockDownFile } from "@shared/infrastructure/storage/lockdown.ts";
import YAML from "yaml";

const LOCAL_PROVIDERS = new Set(["ollama", "lmstudio", "llamacpp", "vllm", "litellm"]);

/** A generic cloud key is not implicit authorization for a local endpoint. */
export function selectSetupKey(
  explicitKey: string,
  isLocal: boolean,
  genericEnvironmentKey?: string,
): string {
  return explicitKey || (isLocal ? "" : genericEnvironmentKey || "");
}

interface ExistingProvider {
  readonly api_key_env?: string;
  readonly models?: readonly string[];
}

interface SetupConfigInput {
  readonly provider: string;
  readonly model: string;
  readonly baseUrl: string;
  readonly constitution: boolean;
  readonly keyConfigured: boolean;
  readonly existing?: ExistingProvider;
}

/** Pure setup-to-config contract, exported so onboarding can lock it under TDD. */
export function setupConfigEntries({
  provider,
  model,
  baseUrl,
  constitution,
  keyConfigured,
  existing,
}: SetupConfigInput): readonly (readonly [string, unknown])[] {
  const models = [...(existing?.models ?? [])].filter((item) => item.trim() !== "");
  if (!models.includes(model)) models.push(model);
  const existingKeyEnv = existing?.api_key_env?.trim();
  const apiKeyEnv = keyConfigured
    ? "REGENT_API_KEY"
    : existingKeyEnv || (LOCAL_PROVIDERS.has(provider) ? "" : "REGENT_API_KEY");

  return [
    ["model.provider", provider],
    ["model.default", model],
    ["model.base_url", baseUrl || null],
    [`providers.${provider}.kind`, provider],
    [`providers.${provider}.base_url`, baseUrl || null],
    [`providers.${provider}.api_key_env`, apiKeyEnv],
    [`providers.${provider}.models`, models],
    ["agents_defaults.primary", { provider, model }],
    ["constitution.enabled", constitution],
  ];
}

function readExistingProvider(home: string, provider: string): ExistingProvider | undefined {
  try {
    const doc = YAML.parse(readFileSync(join(home, "config.yaml"), "utf8")) as {
      providers?: Record<string, ExistingProvider>;
    } | null;
    return doc?.providers?.[provider];
  } catch {
    // The validated writer below reports malformed YAML without changing it.
    return undefined;
  }
}

/** Upsert REGENT_API_KEY in .env, preserving other lines. The temporary file
 * is owner-only before it becomes the live file, so an ACL failure leaves the
 * previous configuration untouched. No key → no write (the caller warns). */
export function writeEnv(
  home: string,
  key: string,
  protect: (path: string) => void = lockDownFile,
): void {
  if (!key) return;
  if (/[\r\n\0]/.test(key)) {
    throw new Error("REGENT_API_KEY must be a single line and cannot contain NUL bytes");
  }
  updateDotenv(join(home, ".env"), { REGENT_API_KEY: key }, protect);
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
  keyConfigured: boolean,
): DeaconConfigResult {
  mkdirSync(home, { recursive: true });
  const existing = readExistingProvider(home, provider);
  return setConfigKeys(
    home,
    setupConfigEntries({
      provider,
      model,
      baseUrl,
      constitution,
      keyConfigured,
      ...(existing ? { existing } : {}),
    }),
  );
}
