// Spawns regent-deacon as a child process (stdio mode) and wires an RpcClient
// to its pipes. Deacon stderr is PIPED and drained into a bounded buffer, never
// left to reach the terminal: it duplicates the deacon's own redacted log file
// ($REGENT_HOME/logs/regent.log.<date>), and any stderr line landing mid-render
// desyncs Ink's frame erase — boot logs left a trail of stacked banners, drain
// logs duplicated the chat input on exit. The bounded snapshot lets a failed
// health probe report the deacon's fatal line (e.g. `fatal: yaml: …`) instead
// of a bare EPIPE. Set REGENT_LOG to also stream stderr to the terminal.
import { type ChildProcess, spawn } from "node:child_process";
import { mkdirSync } from "node:fs";
import type { Writable } from "node:stream";
import { RpcClient } from "@shared/infrastructure/rpc/client.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { type Result, err, failure, ok } from "@shared/kernel/result.ts";
import { buildEnv } from "./env.ts";

// Grace window for the deacon to drain on stdin EOF before we force-kill it.
// A healthy one-shot exits in <100ms; this only fires when the deacon is stuck
// (slow init, AV scan of the freshly-built exe, store-lock deadlock). Without
// it, close() waited on `exit` forever → the CLI hung until an external 60s
// SIGKILL with no output.
const CLOSE_GRACE_MS = 2_000;

// Cap on retained early stderr — enough for a fatal line plus a little context,
// small enough that a long-lived healthy deacon never accumulates its logs.
const STDERR_CAP = 4_096;

export interface SpawnedDeacon {
  // IRpcClient (not the concrete RpcClient) so the candidate-fallback loop can be
  // driven with fake children in tests; production always passes a real RpcClient.
  readonly client: IRpcClient;
  /** Bounded snapshot of the child's stderr so far — the deacon prints its
   *  `fatal: …` reason here before dying, so a failed probe can say WHY. */
  readonly earlyStderr: () => string;
}

/** Create REGENT_HOME (idempotent). Split out so the spawn path and the
 *  candidate-fallback path each ensure it exactly once. */
export function ensureHome(home: string): Result<null> {
  try {
    mkdirSync(home, { recursive: true });
    return ok(null);
  } catch (cause) {
    return err(failure("deacon-spawn", `create REGENT_HOME ${home}`, cause));
  }
}

/**
 * Spawn one deacon binary and wire an RpcClient to its stdio. Does NOT health-
 * probe — the caller does that and closes the returned client if it fails.
 */
export function spawnDeaconChild(deaconPath: string, home: string): Result<SpawnedDeacon> {
  let child: ChildProcess;
  try {
    // stderr piped (not "inherit"/"ignore"): we drain it ourselves so the pipe
    // never blocks the child, keep a bounded snapshot for diagnostics, and keep
    // it off the terminal so Ink's frame erase stays in sync.
    child = spawn(deaconPath, [], { stdio: ["pipe", "pipe", "pipe"], env: buildEnv(home) });
  } catch (cause) {
    return err(failure("deacon-spawn", `spawn deacon ${deaconPath}`, cause));
  }
  if (!child.stdout || !child.stdin || !child.stderr) {
    child.kill();
    return err(failure("deacon-spawn", "deacon stdio pipes were not created"));
  }

  let captured = "";
  const append = (text: string): void => {
    captured = (captured + text).slice(-STDERR_CAP);
  };
  const stream = Boolean(process.env.REGENT_LOG);
  child.stderr.setEncoding("utf8");
  child.stderr.on("data", (chunk: string) => {
    append(chunk);
    if (stream) process.stderr.write(chunk);
  });
  // A force-killed child's stderr pipe can emit EPIPE; swallow it (we're done).
  child.stderr.on("error", () => {});
  // A bogus/non-executable candidate fails to launch AFTER spawn() returns, via
  // an async 'error' event. Without a listener that event throws (uncaught);
  // handle it, record why, and destroy stdout so a pending health probe rejects
  // at once instead of waiting out its timeout.
  child.on("error", (e: Error) => {
    append(`spawn error: ${e.message}\n`);
    child.stdout?.destroy();
  });

  const stdin = child.stdin;
  const client = new RpcClient(child.stdout, stdin, () => closeChild(child, stdin));
  return ok({ client, earlyStderr: () => captured.trim() });
}

/** Graceful shutdown: signal stdin EOF, wait CLOSE_GRACE_MS for a clean drain,
 *  then force-kill so close() never hangs on a stuck deacon. */
function closeChild(child: ChildProcess, stdin: Writable): Promise<void> {
  return new Promise<void>((resolve) => {
    if (
      (child.exitCode !== null || child.signalCode !== null) &&
      child.stdout?.readableEnded &&
      child.stderr?.readableEnded
    ) {
      return resolve();
    }
    let settled = false;
    const finish = (): void => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve();
    };
    child.once("close", finish);
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
  });
}
