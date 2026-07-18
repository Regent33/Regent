// Process lifecycle for the detached `regent-gateway` binary: a PID file under
// $REGENT_HOME tracks the running process; start validates required env first.
import { type ChildProcess, spawn } from "node:child_process";
import { mkdirSync, openSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { out, printError } from "@app/cli/runtime.ts";
import { locateBinary } from "@shared/infrastructure/deacon/locate.ts";
import { style } from "@shared/ui/style.ts";
import { gatewayEnv } from "./gatewayEnv.ts";

const pidPath = (home: string): string => join(home, "gateway.pid");

function isAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

function readPid(home: string): number | null {
  try {
    const pid = Number.parseInt(readFileSync(pidPath(home), "utf8").trim(), 10);
    return Number.isFinite(pid) ? pid : null;
  } catch {
    return null;
  }
}

export function gatewayStatus(home: string): number {
  const pid = readPid(home);
  if (pid !== null && isAlive(pid)) {
    out(`${style.teal("●")} gateway running (pid ${pid})`);
  } else {
    out(`${style.grey("○")} gateway not running`);
    if (pid !== null) rmSync(pidPath(home), { force: true }); // clean a stale pid
  }
  return 0;
}

export function gatewayStart(home: string): number {
  const existing = readPid(home);
  if (existing !== null && isAlive(existing)) {
    out(style.grey(`gateway already running (pid ${existing})`));
    return 0;
  }
  const located = locateBinary("regent-gateway", "REGENT_GATEWAY_PATH");
  if (!located.ok) {
    printError(located.error.message);
    return 1;
  }
  // Validate the gateway's required env up-front — otherwise it spawns, fatals
  // immediately ("REGENT_MODEL not set"), and `status` confusingly shows "not
  // running". Tell the user exactly what to set instead.
  const env = gatewayEnv(home);
  const missing = (
    [
      ["REGENT_TELEGRAM_TOKEN", "regent gateway setup <telegram-token>"],
      ["REGENT_API_KEY", "regent setup  (provider API key)"],
      ["REGENT_MODEL", "regent setup --model <id>  (writes config.yaml)"],
    ] as const
  ).filter(([k]) => !env[k]);
  if (missing.length > 0) {
    printError("gateway can't start — missing configuration:");
    for (const [k, how] of missing) out(`  ${style.fail("✗")} ${k.padEnd(22)} set via: ${how}`);
    return 1;
  }
  mkdirSync(join(home, "logs"), { recursive: true });
  const log = openSync(join(home, "logs", "gateway.log"), "a");
  let child: ChildProcess;
  try {
    child = spawn(located.value, [], {
      detached: true,
      stdio: ["ignore", log, log],
      env,
    });
  } catch (e) {
    printError(`spawn gateway: ${e instanceof Error ? e.message : String(e)}`);
    return 1;
  }
  if (child.pid === undefined) {
    printError("gateway did not start");
    return 1;
  }
  writeFileSync(pidPath(home), String(child.pid));
  child.unref();
  out(
    `gateway started (pid ${style.teal(String(child.pid))}) — logs: ${join(home, "logs", "gateway.log")}`,
  );
  return 0;
}

export function gatewayStop(home: string): number {
  const pid = readPid(home);
  if (pid === null || !isAlive(pid)) {
    out(style.grey("gateway not running"));
    rmSync(pidPath(home), { force: true });
    return 0;
  }
  try {
    process.kill(pid);
  } catch (e) {
    printError(`stop gateway (pid ${pid}): ${e instanceof Error ? e.message : String(e)}`);
    return 1;
  }
  rmSync(pidPath(home), { force: true });
  out(`gateway stopped (pid ${pid})`);
  return 0;
}
