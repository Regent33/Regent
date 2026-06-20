// `regent doctor` — verifies the installation end to end: daemon binary,
// REGENT_HOME, provider key (warn), spawn → health → config.get. Mirrors doctor.go.
import { mkdirSync } from "node:fs";
import { CLI_VERSION } from "@app/cli/help.ts";
import { out, printError } from "@app/cli/runtime.ts";
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

  if (process.env.REGENT_API_KEY) pass("REGENT_API_KEY", "set");
  else warn("REGENT_API_KEY", "not set — prompt.submit will fail until exported");

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
