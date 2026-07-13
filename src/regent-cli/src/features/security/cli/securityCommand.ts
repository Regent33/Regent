// `regent security audit` — a security-focused companion to `doctor`: checks
// $REGENT_HOME, that a provider key is present, and lints config.yaml for
// secret-looking values that belong in .env instead. Pure CLI (filesystem).
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { out, printError } from "@app/cli/runtime.ts";
import { regentHome } from "@shared/infrastructure/deacon/locate.ts";
import { style } from "@shared/ui/style.ts";
import YAML from "yaml";

const pass = (check: string, detail: string): void =>
  out(`  ${style.pass("✓")} ${check.padEnd(20)} ${detail}`);
const warn = (check: string, detail: string): void =>
  out(`  ${style.warn("!")} ${check.padEnd(20)} ${detail}`);
const fail = (check: string, detail: string): void =>
  out(`  ${style.fail("✗")} ${check.padEnd(20)} ${detail}`);

const SECRET_KEY = /key|token|secret|password/i;
// An UPPER_SNAKE name is a REFERENCE to an env var, not a secret itself —
// flagging `api_key_env: OPENROUTER_API_KEY` would be a false positive on
// every properly configured provider.
const ENV_VAR_NAME = /^[A-Z][A-Z0-9_]{2,}$/;

/** True when this key/value pair actually looks like an inlined secret. */
export function looksLikeInlineSecret(key: string, value: string): boolean {
  if (!SECRET_KEY.test(key)) return false;
  // `*_env` keys name the env var that HOLDS the secret — that's the pattern
  // we want users on, never a finding.
  if (/_env$/i.test(key)) return false;
  if (ENV_VAR_NAME.test(value)) return false;
  return value.length > 0;
}

// Collect dotted paths of secret-looking config keys with non-empty string values.
export function scanSecrets(node: unknown, path: string, hits: string[]): void {
  if (!node || typeof node !== "object") return;
  for (const [key, value] of Object.entries(node)) {
    const dotted = path ? `${path}.${key}` : key;
    if (typeof value === "string" && looksLikeInlineSecret(key, value)) {
      hits.push(dotted);
    } else {
      scanSecrets(value, dotted, hits);
    }
  }
}

/** Env var names the config routes provider keys through (`api_key_env`). */
export function configuredKeyVars(config: unknown): string[] {
  const names: string[] = [];
  const providers = (config as { providers?: unknown })?.providers;
  if (providers && typeof providers === "object") {
    for (const entry of Object.values(providers)) {
      const envName = (entry as { api_key_env?: unknown })?.api_key_env;
      if (typeof envName === "string" && envName.length > 0) names.push(envName);
    }
  }
  return names;
}

/** Env var names with a non-empty assignment in the .env file's text. */
export function envFileVars(text: string): Set<string> {
  const found = new Set<string>();
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    const eq = line.indexOf("=");
    if (eq <= 0) continue;
    const key = line.slice(0, eq).trim();
    if (line.slice(eq + 1).trim().length > 0) found.add(key);
  }
  return found;
}

export function securityCommand(profile: string, args: string[]): number {
  if (args[0] && args[0] !== "audit") {
    printError("usage: regent security audit");
    return 1;
  }
  const home = regentHome(profile);
  out(style.heading("regent security audit"));
  let problems = false;

  // 1. REGENT_HOME.
  if (existsSync(home)) pass("REGENT_HOME", home);
  else warn("REGENT_HOME", `${home} (not created yet — run setup)`);

  // Parse config.yaml once — the key check and the secret lint both read it.
  const cfgPath = join(home, "config.yaml");
  let config: unknown;
  let configError: string | undefined;
  if (existsSync(cfgPath)) {
    try {
      config = YAML.parse(readFileSync(cfgPath, "utf8")) as unknown;
    } catch (e) {
      configError = e instanceof Error ? e.message : String(e);
    }
  }

  // 2. Provider key — REGENT_API_KEY or any env var the config's providers
  // route through (api_key_env), present in the environment or .env. Only
  // checking REGENT_API_KEY cried "not set" at every name-keyed provider setup.
  const keyVars = ["REGENT_API_KEY", ...configuredKeyVars(config)];
  const dotEnv = (() => {
    try {
      return envFileVars(readFileSync(join(home, ".env"), "utf8"));
    } catch {
      return new Set<string>();
    }
  })();
  const inEnv = keyVars.filter((name) => (process.env[name] ?? "").length > 0);
  const inFile = keyVars.filter((name) => dotEnv.has(name));
  if (inEnv.length > 0) pass("provider key", `set in environment (${inEnv.join(", ")})`);
  else if (inFile.length > 0) pass("provider key", `set in .env (${inFile.join(", ")})`);
  else warn("provider key", "not set — prompt.submit will fail until exported");

  // 3. config.yaml secret lint: secrets belong in .env, not config.yaml.
  // `api_key_env: SOME_VAR` is the healthy pattern, never flagged.
  if (configError !== undefined) {
    fail("config.yaml", `parse error: ${configError}`);
    problems = true;
  } else if (config === undefined) {
    warn("config.yaml", "absent (defaults in use)");
  } else {
    const hits: string[] = [];
    scanSecrets(config, "", hits);
    if (hits.length === 0) {
      pass("config secrets", "none in config.yaml (secrets stay in .env)");
    } else {
      fail("config secrets", `move these to .env: ${hits.join(", ")}`);
      problems = true;
    }
  }

  if (problems) {
    out("");
    printError("security audit found problems");
    return 1;
  }
  out(`\n${style.pass("no issues found")}`);
  return 0;
}
