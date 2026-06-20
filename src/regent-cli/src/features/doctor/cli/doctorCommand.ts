// `regent doctor` — verifies the installation end to end: daemon binary,
// REGENT_HOME, the EFFECTIVE provider/model/endpoint + active API key (the #1
// cause of HTTP 401), spawn → health → config.get.
import { mkdirSync } from "node:fs";
import { CLI_VERSION } from "@app/cli/help.ts";
import { out, printError } from "@app/cli/runtime.ts";
import { maskKey, readDotEnvKey, readProviderInfo } from "@features/doctor/diagnostics.ts";
import { locateDaemon, regentHome } from "@shared/infrastructure/daemon/locate.ts";
import { connectDaemon } from "@shared/infrastructure/daemon/spawn.ts";
import { style } from "@shared/ui/style.ts";

const pass = (check: string, detail: string): void =>
  out(`  ${style.pass("✓")} ${check.padEnd(18)} ${detail}`);
const warn = (check: string, detail: string): void =>
  out(`  ${style.warn("!")} ${check.padEnd(18)} ${detail}`);
const fail = (check: string, detail: string): void =>
  out(`  ${style.fail("✗")} ${check.padEnd(18)} ${detail}`);

export async function doctorCommand(profile: string): Promise<number> {
  out(`regent doctor (cli ${CLI_VERSION})\n`);
  let hard = false;

  const located = locateDaemon();
  if (!located.ok) {
    fail("daemon binary", located.error.message);
    return 1;
  }
  pass("daemon binary", located.value);

  const home = regentHome(profile);
  try {
    mkdirSync(home, { recursive: true });
    pass("REGENT_HOME", home);
  } catch (e) {
    fail("REGENT_HOME", `${home}: ${e instanceof Error ? e.message : String(e)}`);
    hard = true;
  }

  // Effective provider/model/endpoint (read straight from config.yaml).
  const { provider, model, endpoint } = readProviderInfo(home);
  pass("provider", `${provider} · ${model} · ${endpoint}`);

  // Active API key: a shell-exported REGENT_API_KEY OVERRIDES .env (real env
  // wins), which is the usual reason a fresh `setup` key still 401s.
  const envKey = process.env.REGENT_API_KEY?.trim();
  const dotenvKey = readDotEnvKey(home);
  const activeKey = envKey || dotenvKey;
  if (!activeKey) {
    fail("API key", "no REGENT_API_KEY in shell env or .env — run `regent setup`");
    hard = true;
  } else {
    pass("API key", `${maskKey(activeKey)} (from ${envKey ? "shell env" : ".env"})`);
    if (envKey && dotenvKey && envKey !== dotenvKey) {
      warn(
        "API key",
        "a shell-exported REGENT_API_KEY is OVERRIDING your .env key — unset it (PowerShell: `Remove-Item Env:REGENT_API_KEY`; bash: `unset REGENT_API_KEY`) to use the key from setup",
      );
    }
  }

  const connected = connectDaemon(located.value, home);
  if (!connected.ok) {
    fail("daemon spawn", connected.error.message);
    return 1;
  }
  const client = connected.value;

  const health = await client.call("health", {}, 15_000);
  if (health.ok) pass("health round-trip", "ok");
  else {
    fail("health round-trip", health.error.message);
    hard = true;
  }

  const cfg = await client.call("config.get", {}, 15_000);
  if (cfg.ok) pass("config.yaml", "loads and validates");
  else {
    fail("config.yaml", cfg.error.message);
    hard = true;
  }
  await client.close();

  if (hard) {
    printError("doctor found problems");
    return 1;
  }
  out(`\n${style.pass("all checks passed")}`);
  return 0;
}
