// `regent doctor` — verifies the installation end to end: deacon binary,
// REGENT_HOME, the EFFECTIVE provider/model/endpoint + active API key (the #1
// cause of HTTP 401), spawn → health → config.get.
import { mkdirSync } from "node:fs";
import { CLI_VERSION } from "@app/cli/help.ts";
import { out, printError } from "@app/cli/runtime.ts";
import { maskKey, readDotEnvKey, readProviderInfo } from "@features/doctor/diagnostics.ts";
import { checkForUpdateNotice } from "@features/update/data/checkForUpdate.ts";
import { connectHealthyDeacon } from "@shared/infrastructure/deacon/connect.ts";
import { deaconCandidates, regentHome } from "@shared/infrastructure/deacon/locate.ts";
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

  const candidates = deaconCandidates();
  if (candidates.length === 0) {
    fail("deacon binary", "not found (set REGENT_DEACON_PATH or build regent-deacon)");
    return 1;
  }
  const [firstCandidate] = candidates;
  pass("deacon binary", `${candidates.length} candidate(s); first: ${firstCandidate}`);

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

  // Spawn + health-probe every candidate; report the one that actually answered
  // (which may not be the first if a stale pinned binary boots then dies).
  const connected = await connectHealthyDeacon(home);
  if (!connected.ok) {
    fail("health round-trip", connected.error.message);
    return 1;
  }
  const client = connected.value.client;
  pass("health round-trip", `ok via ${connected.value.path}`);

  const cfg = await client.call("config.get", {}, 15_000);
  if (cfg.ok) pass("config.yaml", "loads and validates");
  else {
    fail("config.yaml", cfg.error.message);
    hard = true;
  }

  // Notify-only update check (ADR-041, Phase 0): informational, never a failure.
  // A missing method (old deacon) or any error yields no line — doctor's verdict
  // is unaffected.
  const updateNotice = await checkForUpdateNotice(client, CLI_VERSION);
  await client.close();
  if (updateNotice) warn("update", updateNotice);

  if (hard) {
    printError("doctor found problems");
    return 1;
  }
  out(`\n${style.pass("all checks passed")}`);
  return 0;
}
