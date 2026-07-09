// Spawns regent-deacon as a child process (stdio mode) and wires an RpcClient
// to its pipes. Merges $REGENT_HOME/.env for secrets (the real environment
// always wins, so .env never overrides an explicit export) — mirrors the Go
// appendDotEnv. Deacon stderr is inherited so its logs stay visible.
import { type ChildProcess, spawn } from "node:child_process";
import { mkdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { RpcClient } from "@shared/infrastructure/rpc/client.ts";
import { type Result, err, failure, ok } from "@shared/kernel/result.ts";

// Grace window for the deacon to drain on stdin EOF before we force-kill it.
// A healthy one-shot exits in <100ms; this only fires when the deacon is stuck
// (slow init, AV scan of the freshly-built exe, store-lock deadlock). Without
// it, close() waited on `exit` forever → the CLI hung until an external 60s
// SIGKILL with no output.
const CLOSE_GRACE_MS = 2_000;

/** Spawn the deacon for `home` and return a connected client. */
export function connectDeacon(deaconPath: string, home: string): Result<RpcClient> {
  try {
    mkdirSync(home, { recursive: true });
  } catch (cause) {
    return err(failure("deacon-spawn", `create REGENT_HOME ${home}`, cause));
  }

  let child: ChildProcess;
  try {
    child = spawn(deaconPath, [], { stdio: ["pipe", "pipe", "inherit"], env: buildEnv(home) });
  } catch (cause) {
    return err(failure("deacon-spawn", `spawn deacon ${deaconPath}`, cause));
  }
  if (!child.stdout || !child.stdin) {
    return err(failure("deacon-spawn", "deacon stdio pipes were not created"));
  }

  const stdin = child.stdin;
  const client = new RpcClient(
    child.stdout,
    stdin,
    () =>
      new Promise<void>((resolve) => {
        if (child.exitCode !== null || child.signalCode !== null) return resolve();
        let settled = false;
        const finish = () => {
          if (settled) return;
          settled = true;
          clearTimeout(timer);
          resolve();
        };
        child.once("exit", finish);
        // EOF → deacon drains and exits. If it doesn't within the grace window,
        // force-kill so the CLI never hangs (bounded shutdown, not infinite wait).
        const timer = setTimeout(() => {
          child.kill();
          finish();
        }, CLOSE_GRACE_MS);
        try {
          stdin.end();
        } catch {
          // stdin already gone — the exit/timeout race still settles us.
        }
      }),
  );
  return ok(client);
}

function buildEnv(home: string): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = { ...process.env, REGENT_HOME: home };
  for (const [key, value] of readDotEnv(home)) {
    if (env[key] === undefined) env[key] = value;
  }
  // The deacon has no clock dep; hand it the wall-clock so the agent can answer
  // date/time. Set at spawn (a fresh deacon per `regent` invocation = current).
  env.REGENT_NOW = new Date().toLocaleString();
  // Desktop control available by default in the CLI, matching the voice call's
  // default — safe here because every mutating action still asks in the TUI
  // (interactive approval). REGENT_COMPUTER_USE=0 in env/.env disables it.
  if (env.REGENT_COMPUTER_USE === undefined) env.REGENT_COMPUTER_USE = "1";
  return env;
}

/** Parse $REGENT_HOME/.env into key/value pairs, ignoring blanks and comments. */
function readDotEnv(home: string): Array<[string, string]> {
  let data: string;
  try {
    data = readFileSync(join(home, ".env"), "utf8");
  } catch {
    return [];
  }
  const out: Array<[string, string]> = [];
  for (const raw of data.split("\n")) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    const eq = line.indexOf("=");
    if (eq <= 0) continue;
    const key = line.slice(0, eq).trim();
    const value = line
      .slice(eq + 1)
      .trim()
      .replace(/^"|"$/g, "");
    out.push([key, value]);
  }
  return out;
}
