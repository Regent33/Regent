// Reading a secret from stdin — the one intake shared by the commands that take
// one. A token passed as an argv value is already in the shell's history file
// and readable in `ps`/Task Manager output before the process even starts, so
// the pipe is not a nicety: it is the only intake that does not leak.
import { readFileSync } from "node:fs";

/** Injectable so tests need no real pipe; production reads fd 0. */
export type ReadSecret = () => string;

export const readStdin: ReadSecret = () => readFileSync(0, "utf8");

/**
 * One secret from `read`, minus the single line ending a normal pipe adds.
 * Throws when the value is empty or could carry a second `KEY=VALUE` line into
 * `.env` — `updateDotenv` refuses those too, but this message names the input
 * the user actually typed instead of the file it would have corrupted.
 */
export function secretFromStdin(read: ReadSecret): string {
  const value = read().replace(/\r?\n$/, "");
  if (/[\r\n\0]/.test(value)) {
    throw new Error("a secret must be one line and cannot contain NUL bytes");
  }
  const trimmed = value.trim();
  if (trimmed === "") throw new Error("stdin did not contain a value");
  return trimmed;
}
