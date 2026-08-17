// Cross-process serialization for the one shared Regent `.env` file.
//
// Atomic replacement prevents a torn file, but it does not make a
// read-modify-replace sequence atomic: two processes can read the same old
// contents and the later rename silently discards the earlier process's key.
// A same-directory lock DIRECTORY is the smallest primitive Rust and Bun can
// both acquire atomically on Windows and Unix without another dependency.
import { mkdirSync, rmSync } from "node:fs";
import { dirname } from "node:path";

const LOCK_TIMEOUT_MS = 10_000;
const RETRY_MS = 20;
const sleeper = new Int32Array(new SharedArrayBuffer(4));

function sleep(ms: number): void {
  Atomics.wait(sleeper, 0, 0, ms);
}

function errorCode(error: unknown): string | undefined {
  return typeof error === "object" && error !== null && "code" in error
    ? String((error as { code?: unknown }).code)
    : undefined;
}

/** Hold `<dotenv>.lock/` around the complete read-modify-publish operation. */
export function withDotenvLock<T>(dotenvPath: string, operation: () => T): T {
  mkdirSync(dirname(dotenvPath), { recursive: true });
  const lockPath = `${dotenvPath}.lock`;
  const started = Date.now();

  for (;;) {
    try {
      mkdirSync(lockPath, { mode: 0o700 });
      break;
    } catch (error) {
      if (errorCode(error) !== "EEXIST") throw error;

      if (Date.now() - started >= LOCK_TIMEOUT_MS) {
        throw new Error(
          `timed out waiting for the Regent credential lock: ${lockPath}; if no Regent process is saving settings, remove that empty lock directory and retry`,
        );
      }
      sleep(RETRY_MS);
    }
  }

  try {
    return operation();
  } finally {
    rmSync(lockPath, { recursive: true, force: true });
  }
}
