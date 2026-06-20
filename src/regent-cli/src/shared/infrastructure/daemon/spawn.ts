// Spawns regent-daemon as a child process (stdio mode) and wires an RpcClient
// to its pipes. Merges $REGENT_HOME/.env for secrets (the real environment
// always wins, so .env never overrides an explicit export) — mirrors the Go
// appendDotEnv. Daemon stderr is inherited so its logs stay visible.
import { type ChildProcess, spawn } from "node:child_process";
import { mkdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { RpcClient } from "@shared/infrastructure/rpc/client.ts";
import { type Result, err, failure, ok } from "@shared/kernel/result.ts";

/** Spawn the daemon for `home` and return a connected client. */
export function connectDaemon(daemonPath: string, home: string): Result<RpcClient> {
  try {
    mkdirSync(home, { recursive: true });
  } catch (cause) {
    return err(failure("daemon-spawn", `create REGENT_HOME ${home}`, cause));
  }

  let child: ChildProcess;
  try {
    child = spawn(daemonPath, [], { stdio: ["pipe", "pipe", "inherit"], env: buildEnv(home) });
  } catch (cause) {
    return err(failure("daemon-spawn", `spawn daemon ${daemonPath}`, cause));
  }
  if (!child.stdout || !child.stdin) {
    return err(failure("daemon-spawn", "daemon stdio pipes were not created"));
  }

  const stdin = child.stdin;
  const client = new RpcClient(
    child.stdout,
    stdin,
    () =>
      new Promise<void>((resolve) => {
        if (child.exitCode !== null || child.signalCode !== null) return resolve();
        child.once("exit", () => resolve());
        stdin.end(); // EOF → daemon drains and exits
      }),
  );
  return ok(client);
}

function buildEnv(home: string): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = { ...process.env, REGENT_HOME: home };
  for (const [key, value] of readDotEnv(home)) {
    if (env[key] === undefined) env[key] = value;
  }
  // The daemon has no clock dep; hand it the wall-clock so the agent can answer
  // date/time. Set at spawn (a fresh daemon per `regent` invocation = current).
  env.REGENT_NOW = new Date().toLocaleString();
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
