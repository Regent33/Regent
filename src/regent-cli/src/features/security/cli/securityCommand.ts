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

// Collect dotted paths of secret-looking config keys with non-empty string values.
function scanSecrets(node: unknown, path: string, hits: string[]): void {
  if (!node || typeof node !== "object") return;
  for (const [key, value] of Object.entries(node)) {
    const dotted = path ? `${path}.${key}` : key;
    if (typeof value === "string" && value.length > 0 && SECRET_KEY.test(key)) {
      hits.push(dotted);
    } else {
      scanSecrets(value, dotted, hits);
    }
  }
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

  // 2. Provider key — present, and only via env/.env (never committed in code).
  const envFile = join(home, ".env");
  const hasEnvKey = (() => {
    try {
      return /^\s*REGENT_API_KEY\s*=\s*\S/m.test(readFileSync(envFile, "utf8"));
    } catch {
      return false;
    }
  })();
  if (process.env.REGENT_API_KEY) pass("provider key", "set in environment");
  else if (hasEnvKey) pass("provider key", "set in .env");
  else warn("provider key", "not set — prompt.submit will fail until exported");

  // 3. config.yaml secret lint: secrets belong in .env, not config.yaml.
  const cfgPath = join(home, "config.yaml");
  if (existsSync(cfgPath)) {
    try {
      const doc = YAML.parse(readFileSync(cfgPath, "utf8")) as unknown;
      const hits: string[] = [];
      scanSecrets(doc, "", hits);
      if (hits.length === 0) {
        pass("config secrets", "none in config.yaml (secrets stay in .env)");
      } else {
        fail("config secrets", `move these to .env: ${hits.join(", ")}`);
        problems = true;
      }
    } catch (e) {
      fail("config.yaml", `parse error: ${e instanceof Error ? e.message : String(e)}`);
      problems = true;
    }
  } else {
    warn("config.yaml", "absent (defaults in use)");
  }

  if (problems) {
    out("");
    printError("security audit found problems");
    return 1;
  }
  out(`\n${style.pass("no issues found")}`);
  return 0;
}
