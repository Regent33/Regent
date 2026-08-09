// Black-box regression tests for the COMPILED CLI (Phase A.1 of the CLI
// superiority plan). They record the shell contract — exit code, stdout and
// stderr kept separate — of the built artefact on Windows and Linux, so later
// phases cannot change it by accident.
//
// Deacon-free paths only. Anything that spawns regent-deacon depends on a Rust
// build that CI's `cli` job does not have, and would be flaky rather than
// protective. Deacon lifecycle assertions belong to Phase D.
//
// Skipped when dist/regent-cli(.exe) hasn't been built (`bun run compile`).
import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { SETUP_MARKER } from "@features/setup/domain/firstRun.ts";

const exe = join(
  import.meta.dir,
  "../../..",
  "dist",
  process.platform === "win32" ? "regent-cli.exe" : "regent-cli",
);
const built = existsSync(exe);
// Skipping is for a developer who hasn't compiled. In CI the compile step runs
// first, so a missing artefact means the build moved — fail, never skip green.
if (!built && process.env.CI) throw new Error(`compiled CLI missing in CI: ${exe}`);

// A 95 MB binary under Windows Defender is slow to start; no tight timings.
const TIMEOUT_MS = 15_000;

interface Run {
  readonly code: number | null;
  readonly out: string;
  readonly err: string;
  /** True when the binary had to be killed — i.e. it hung. */
  readonly hung: boolean;
}

/** Run the compiled CLI against a throwaway home, capturing the streams apart. */
function run(args: string[], opts: { home?: string; stdin?: string } = {}): Run {
  const home = opts.home ?? freshHome();
  const env: Record<string, string | undefined> = {
    ...process.env,
    REGENT_HOME: home,
    NO_COLOR: "1",
  };
  // A developer with the escape hatch exported would otherwise re-open the very
  // hang these tests exist to prevent. (Covered at unit level in
  // interactive.test.ts, where no real terminal is needed.)
  delete env.REGENT_FORCE_TTY;
  const r = Bun.spawnSync([exe, ...args], {
    env,
    stdin: new TextEncoder().encode(opts.stdin ?? ""),
    timeout: TIMEOUT_MS,
  });
  return {
    code: r.exitCode,
    out: r.stdout.toString(),
    err: r.stderr.toString(),
    hung: r.exitCode === null,
  };
}

/** A home that has already been through onboarding, so the wizard stays out. */
function freshHome(): string {
  const h = mkdtempSync(join(tmpdir(), "regent-cli-int-"));
  writeFileSync(join(h, SETUP_MARKER), "test\n");
  return h;
}

describe.skipIf(!built)("compiled CLI — shell contract", () => {
  test(
    "--version and version both print to stdout and exit 0",
    () => {
      for (const args of [["--version"], ["-v"], ["version"]]) {
        const r = run(args);
        expect(r.code).toBe(0);
        expect(r.out).toMatch(/^regent \d+\.\d+\.\d+/);
        expect(r.err).toBe("");
      }
    },
    TIMEOUT_MS * 3,
  );

  test("--help and help print usage to stdout, nothing to stderr, exit 0", () => {
    for (const args of [["--help"], ["-h"], ["help"]]) {
      const r = run(args);
      expect(r.code).toBe(0);
      expect(r.out).toContain("Usage");
      expect(r.out).toContain("regent <command> [args]");
      expect(r.err).toBe("");
    }
  });

  test("unknown command: diagnostic on stderr, usage exit code, nothing on stdout", () => {
    const r = run(["nosuchcmd"]);
    expect(r.code).toBe(2);
    expect(r.err).toContain("unknown command: nosuchcmd");
    // A failed invocation must not pollute stdout — a caller piping us gets
    // the empty result it deserves, and the help goes where diagnostics go.
    expect(r.out).toBe("");
    expect(r.err).toContain("Usage");
  });

  test("unknown option is diagnosed as an option, not as a command", () => {
    const r = run(["--nosuchopt"]);
    expect(r.code).toBe(2);
    expect(r.err).toContain("unknown option: --nosuchopt");
    expect(r.out).toBe("");
  });

  test("a lone dash and a negative-looking token are not treated as options", () => {
    // `-` means stdin by convention and must not be reported as an option.
    expect(run(["-"]).err).not.toContain("unknown option");
    expect(run(["-5"]).err).not.toContain("unknown option");
  });

  test("`--` ends option parsing", () => {
    const r = run(["--", "--nosuchopt"]);
    expect(r.err).not.toContain("unknown option");
  });

  test("<command> --help answers locally on stdout without a deacon", () => {
    for (const cmd of ["code", "memory", "config", "doctor"]) {
      const r = run([cmd, "--help"]);
      expect(r.code).toBe(0);
      expect(r.out).toContain(`regent ${cmd}`);
      expect(r.err).toBe("");
      expect(r.hung).toBe(false);
    }
  });

  test("usage errors exit 2 and write only to stderr", () => {
    const r = run(["config", "set"]);
    expect(r.code).toBe(2);
    expect(r.err).toContain("usage:");
    expect(r.out).toBe("");
  });

  test("bare invocation with piped stdin exits instead of hanging", () => {
    // Audit probe 10: this rendered the banner and hung forever. The guard has
    // to fire before Ink starts and before the deacon is spawned.
    const r = run([], { stdin: "summarise this\n" });
    expect(r.hung).toBe(false);
    expect(r.code).toBe(2);
    expect(r.err).toContain("not a terminal");
    expect(r.out).toBe("");
  });

  test("bare invocation with immediately-closed stdin exits instead of hanging", () => {
    const r = run([], { stdin: "" });
    expect(r.hung).toBe(false);
    expect(r.code).toBe(2);
    expect(r.err).toContain("not a terminal");
  });

  test("`chat` with piped stdin gets the same guard as the bare form", () => {
    const r = run(["chat"], { stdin: "" });
    expect(r.hung).toBe(false);
    expect(r.code).toBe(2);
  });

  test("a missing --profile value is a usage error, not a profile named --help", () => {
    for (const args of [["--profile"], ["-p"], ["--profile", "--help"], ["--profile="]]) {
      const r = run(args);
      expect(r.code).toBe(2);
      expect(r.err).toContain("requires a profile name");
      expect(r.out).toBe("");
    }
  });

  test("the guard fires before onboarding, on a home that never ran setup", () => {
    const virgin = mkdtempSync(join(tmpdir(), "regent-cli-virgin-"));
    const r = run([], { home: virgin, stdin: "" });
    expect(r.hung).toBe(false);
    expect(r.code).toBe(2);
    // The wizard prompts for a provider; it must never be reached non-interactively.
    expect(r.out).not.toContain("Regent Setup");
  });
});
