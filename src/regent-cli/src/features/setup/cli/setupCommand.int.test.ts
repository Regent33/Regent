// Integration tests for the first-run onboarding wizard: spawns the compiled
// CLI against a throwaway REGENT_HOME, so the live ~/.regent is never touched.
// Skipped when dist/regent-cli(.exe) hasn't been built (`bun run compile`).
import { afterEach, describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import YAML from "yaml";

const exe = join(
  import.meta.dir,
  "../../../..",
  "dist",
  process.platform === "win32" ? "regent-cli.exe" : "regent-cli",
);
const built = existsSync(exe);

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
    env: { ...process.env, REGENT_HOME: home },
    stdin: new TextEncoder().encode(stdin),
  });
  return { code: r.exitCode, out: r.stdout.toString() + r.stderr.toString() };
}

function readConfig(home: string): Record<string, unknown> {
  return YAML.parse(readFileSync(join(home, "config.yaml"), "utf8"));
}

describe.skipIf(!built)("onboarding wizard (compiled CLI, sandboxed home)", () => {
  test("non-interactive run falls back to defaults, exit 0, constitution on", () => {
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
  });

  test("unknown provider is rejected with exit 1 and writes nothing", () => {
    const home = freshHome();
    const { code, out } = runSetup(home, ["--provider", "notreal"]);
    expect(code).toBe(1);
    expect(out).toContain("unknown provider");
    expect(existsSync(join(home, "config.yaml"))).toBe(false);
  });

  test("flag-driven run is fully non-interactive (ollama needs no key)", () => {
    const home = freshHome();
    const { code } = runSetup(home, ["--provider", "ollama", "--model", "llama3.2"]);
    expect(code).toBe(0);
    const cfg = readConfig(home) as { model: { provider: string; default: string } };
    expect(cfg.model.provider).toBe("ollama");
    expect(cfg.model.default).toBe("llama3.2");
  });

  test("--key lands in .env as REGENT_API_KEY, never in config.yaml", () => {
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
  });

  test("re-running setup switches provider but preserves unrelated config keys", () => {
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
  });

  // Found 2026-07-14: any command that boots the deacon (e.g. `regent model list`)
  // seeds a full config.yaml, so the `existsSync(config.yaml)` first-run gate in
  // router.ts is defeated and the wizard never appears — the user lands in chat
  // with no key and no guidance. Gate should use a wizard-written marker instead.
  test.todo("first-run wizard still appears after a deacon-booting command seeded config.yaml");
});
