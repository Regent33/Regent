// Integration tests for the first-run onboarding wizard: spawns the compiled
// CLI against a throwaway REGENT_HOME, so the live ~/.regent is never touched.
// Skipped when dist/regent-cli(.exe) hasn't been built (`bun run compile`).
import { afterEach, describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import YAML from "yaml";

const EXE = process.platform === "win32" ? ".exe" : "";
const exe = join(import.meta.dir, "../../../..", "dist", `regent-cli${EXE}`);
const built = existsSync(exe);
// CI compiles before testing, so a missing artefact there is a build change,
// not a developer who skipped `bun run compile` — fail rather than skip green.
if (!built && process.env.CI) throw new Error(`compiled CLI missing in CI: ${exe}`);

// Setup writes config.yaml through `regent-deacon config set` — the one
// validated, locked, atomic writer — so these need a Rust build too. A pinned
// REGENT_DEACON_PATH is deliberately IGNORED here: an older installed deacon
// has no `config` subcommand, and the point is to test this repo's.
const root = join(import.meta.dir, "../../../../../..");
const deacon = ["release", "debug"]
  .map((p) => join(root, "target", p, `regent-deacon${EXE}`))
  .find(existsSync);
// One CI leg builds the deacon and sets this, so the config path is proven on
// every run rather than quietly skipped everywhere.
if (!deacon && process.env.REGENT_CI_EXPECT_DEACON) {
  throw new Error(`regent-deacon missing where CI expects it: ${root}/target/*/regent-deacon`);
}
const full = built && deacon !== undefined;

// Each case spawns the compiled CLI, which spawns the deacon; bun's 5s default
// is a coin flip against a cold binary on a shared CI runner.
const SLOW = 60_000;

const homes: string[] = [];
function freshHome(): string {
  const h = mkdtempSync(join(tmpdir(), "regent-onb-"));
  homes.push(h);
  return h;
}
afterEach(() => {
  for (const h of homes.splice(0)) rmSync(h, { recursive: true, force: true });
});

function runSetup(home: string, args: string[], stdin = "") {
  const r = Bun.spawnSync([exe, "setup", ...args], {
    env: { ...process.env, REGENT_HOME: home, REGENT_DEACON_PATH: deacon ?? "" },
    stdin: new TextEncoder().encode(stdin),
  });
  return { code: r.exitCode, out: r.stdout.toString() + r.stderr.toString() };
}

function readConfig(home: string): Record<string, unknown> {
  return YAML.parse(readFileSync(join(home, "config.yaml"), "utf8"));
}

describe.skipIf(!built)("onboarding wizard — rejections (no deacon needed)", () => {
  test(
    "unknown provider is rejected with exit 1 and writes nothing",
    () => {
      const home = freshHome();
      const { code, out } = runSetup(home, ["--provider", "notreal"]);
      expect(code).toBe(1);
      expect(out).toContain("unknown provider");
      expect(existsSync(join(home, "config.yaml"))).toBe(false);
    },
    SLOW,
  );
});

describe.skipIf(!full)("onboarding wizard (compiled CLI, sandboxed home)", () => {
  test(
    "non-interactive run falls back to defaults, exit 0, constitution on",
    () => {
      const home = freshHome();
      const { code, out } = runSetup(home, [], "\n\n\n\n");
      expect(code).toBe(0);
      expect(out).toContain("Setup complete");
      expect(out).toContain("not set"); // warns about the missing API key
      const cfg = readConfig(home) as { model: { provider: string; default: string } } & {
        constitution: { enabled: boolean };
      };
      expect(cfg.model.provider).toBe("anthropic");
      expect(cfg.model.default).toBe("claude-sonnet-4-6");
      expect(cfg.constitution.enabled).toBe(true);
      expect(existsSync(join(home, ".env"))).toBe(false); // no key → no .env
    },
    SLOW,
  );

  test(
    "flag-driven run is fully non-interactive (ollama needs no key)",
    () => {
      const home = freshHome();
      const { code } = runSetup(home, ["--provider", "ollama", "--model", "llama3.2"]);
      expect(code).toBe(0);
      const cfg = readConfig(home) as { model: { provider: string; default: string } };
      expect(cfg.model.provider).toBe("ollama");
      expect(cfg.model.default).toBe("llama3.2");
    },
    SLOW,
  );

  test(
    "every self-hosted OpenAI-compatible provider is key-optional",
    () => {
      const home = freshHome();
      const { code, out } = runSetup(home, [
        "--provider",
        "lmstudio",
        "--model",
        "local-model",
        "--base-url",
        "http://localhost:1234",
      ]);
      expect(code).toBe(0);
      expect(out).toContain("optional");
      expect(out).not.toContain("warning: no API key set");
      expect(existsSync(join(home, ".env"))).toBe(false);
      expect(readConfig(home)).toMatchObject({
        model: {
          provider: "lmstudio",
          default: "local-model",
          base_url: "http://localhost:1234",
        },
        providers: {
          lmstudio: {
            kind: "lmstudio",
            base_url: "http://localhost:1234",
            api_key_env: "",
            models: ["local-model"],
          },
        },
        agents_defaults: {
          primary: { provider: "lmstudio", model: "local-model" },
        },
      });
    },
    SLOW,
  );

  test(
    "--key lands in .env as REGENT_API_KEY, never in config.yaml",
    () => {
      const home = freshHome();
      const { code } = runSetup(home, [
        ...["--provider", "anthropic"],
        ...["--model", "claude-sonnet-4-6"],
        ...["--key", "sk-ant-test-not-a-real-key"],
      ]);
      expect(code).toBe(0);
      expect(readFileSync(join(home, ".env"), "utf8")).toContain(
        "REGENT_API_KEY=sk-ant-test-not-a-real-key",
      );
      expect(readFileSync(join(home, "config.yaml"), "utf8")).not.toContain("sk-ant-");
    },
    SLOW,
  );

  test(
    "re-running setup switches provider but preserves unrelated config keys",
    () => {
      const home = freshHome();
      runSetup(home, ["--provider", "anthropic", "--model", "claude-sonnet-4-6"]);
      const before = readConfig(home);
      before.cron = { tick_interval_secs: 99 }; // unrelated key a user/deacon added
      writeFileSync(join(home, "config.yaml"), YAML.stringify(before));
      const { code } = runSetup(home, ["--provider", "ollama", "--model", "llama3.2"]);
      expect(code).toBe(0);
      const after = readConfig(home) as {
        model: { provider: string };
        cron: { tick_interval_secs: number };
      };
      expect(after.model.provider).toBe("ollama");
      expect(after.cron.tick_interval_secs).toBe(99);
    },
    SLOW,
  );

  // The bug this replaced: setup parsed config.yaml itself and, on a parse
  // error, "started fresh" — one bad line and re-running setup silently
  // replaced the whole file, providers and all. Byte-for-byte equality is the
  // assertion, because anything less would pass if setup rewrote it "helpfully".
  test(
    "a malformed config.yaml is reported, not overwritten",
    () => {
      const home = freshHome();
      const path = join(home, "config.yaml");
      const broken = "model:\n  provider: anthropic\n    default: oops\nproviders: {\n";
      writeFileSync(path, broken);
      const { code, out } = runSetup(home, ["--provider", "ollama", "--model", "llama3.2"]);
      expect(code).toBe(1);
      expect(out).toContain("not valid YAML");
      expect(readFileSync(path, "utf8")).toBe(broken);
      expect(existsSync(join(home, ".setup-done"))).toBe(false); // and setup is not "done"
    },
    SLOW,
  );

  test(
    "a provider the deacon does not know is refused before it can be written",
    () => {
      const home = freshHome();
      runSetup(home, ["--provider", "ollama", "--model", "llama3.2"]);
      // Straight past the CLI's own list, the way a stale flag or script would.
      // The schema is the last gate, and it has to hold on its own.
      const r = Bun.spawnSync([deacon ?? "", "config", "set", "model.provider", "notaprovider"], {
        env: { ...process.env, REGENT_HOME: home },
        stdin: new Uint8Array(),
      });
      expect(r.exitCode).toBe(1);
      expect(r.stdout.toString()).toContain("invalid");
      expect(readConfig(home)).toMatchObject({ model: { provider: "ollama" } });
    },
    SLOW,
  );

  // The deacon-seeded-config gate bug (found 2026-07-14) is fixed by the
  // marker-based gate in ../domain/firstRun.ts — unit-tested in firstRun.test.ts.
  test(
    "wizard completion writes the .setup-done marker",
    () => {
      const home = freshHome();
      const { code } = runSetup(home, ["--provider", "ollama", "--model", "llama3.2"]);
      expect(code).toBe(0);
      expect(existsSync(join(home, ".setup-done"))).toBe(true);
    },
    SLOW,
  );
});
