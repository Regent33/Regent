// The other half of the TTY decision. The compiled-binary suite can only prove
// that a pipe is refused — it has no PTY to prove that a real terminal is
// accepted, and wrongly refusing an interactive user is the worse failure. So
// the descriptor combinations are exercised here instead.
import { afterEach, expect, test } from "bun:test";
import { EXIT } from "./exit.ts";
import { refuseNonInteractive } from "./interactive.ts";

const original = {
  stdin: process.stdin.isTTY,
  stdout: process.stdout.isTTY,
  force: process.env.REGENT_FORCE_TTY,
  write: process.stderr.write,
};

function setTty(stdin: boolean, stdout: boolean): void {
  Object.defineProperty(process.stdin, "isTTY", { value: stdin, configurable: true });
  Object.defineProperty(process.stdout, "isTTY", { value: stdout, configurable: true });
}

/** Run the guard with stderr captured so the suite output stays clean. */
function guard(): { code: number | null; err: string } {
  let err = "";
  process.stderr.write = ((chunk: string) => {
    err += chunk;
    return true;
  }) as typeof process.stderr.write;
  try {
    return { code: refuseNonInteractive(), err };
  } finally {
    process.stderr.write = original.write;
  }
}

afterEach(() => {
  Object.defineProperty(process.stdin, "isTTY", { value: original.stdin, configurable: true });
  Object.defineProperty(process.stdout, "isTTY", { value: original.stdout, configurable: true });
  if (original.force === undefined) delete process.env.REGENT_FORCE_TTY;
  else process.env.REGENT_FORCE_TTY = original.force;
  process.stderr.write = original.write;
});

test("a real terminal on both ends is allowed through", () => {
  setTty(true, true);
  expect(guard().code).toBeNull();
});

test("either end piped is refused with the usage exit code", () => {
  for (const [stdin, stdout] of [
    [false, true],
    [true, false],
    [false, false],
  ] as const) {
    setTty(stdin, stdout);
    const r = guard();
    expect(r.code).toBe(EXIT.usage);
    expect(r.err).toContain("not a terminal");
  }
});

test("the message names the end that is not a terminal", () => {
  setTty(false, true);
  expect(guard().err).toContain("stdin is not a terminal");
  setTty(true, false);
  expect(guard().err).toContain("stdout is not a terminal");
});

test("REGENT_FORCE_TTY=1 lets a misdetected terminal through", () => {
  setTty(false, false);
  process.env.REGENT_FORCE_TTY = "1";
  expect(guard().code).toBeNull();
});

test("only the exact value 1 opts out — a stray empty value must not disable it", () => {
  setTty(false, false);
  for (const v of ["", "0", "false", "yes"]) {
    process.env.REGENT_FORCE_TTY = v;
    expect(guard().code).toBe(EXIT.usage);
  }
});
