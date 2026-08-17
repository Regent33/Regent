// `gateway setup` takes its bot token from a pipe only. These cover both halves
// of that: the stdin path really stores the token, and every argv path that used
// to work is refused *before* anything is written.
import { afterEach, describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { gatewayCommand } from "./gatewayCommand.ts";

let home = "";
const priorHome = process.env.REGENT_HOME;

function freshHome(): string {
  home = mkdtempSync(join(tmpdir(), "regent-gw-"));
  process.env.REGENT_HOME = home;
  return home;
}

const envText = (): string => readFileSync(join(home, ".env"), "utf8");
const hasEnv = (): boolean => existsSync(join(home, ".env"));

afterEach(() => {
  if (home) rmSync(home, { recursive: true, force: true });
  home = "";
  if (priorHome === undefined) delete process.env.REGENT_HOME;
  else process.env.REGENT_HOME = priorHome;
});

describe("regent gateway setup", () => {
  test("--token-stdin stores the piped token, the platform, and the allowlist", () => {
    freshHome();
    const code = gatewayCommand(
      "",
      ["setup", "telegram", "--token-stdin", "--allowed-users", "4095", "--no-start"],
      () => "8910084620:AAEDy-piped\n",
    );
    expect(code).toBe(0);
    expect(envText()).toContain("REGENT_TELEGRAM_TOKEN=8910084620:AAEDy-piped");
    expect(envText()).toContain("REGENT_GATEWAY_PLATFORM=telegram");
    expect(envText()).toContain("REGENT_TELEGRAM_ALLOWED_USERS=4095");
    expect(envText()).not.toContain("REGENT_TELEGRAM_ALLOW_ALL");
  });

  test("--token-stdin works for a platform the gateway cannot run yet", () => {
    freshHome();
    expect(gatewayCommand("", ["setup", "slack", "--token-stdin"], () => "xoxb-piped\n")).toBe(0);
    expect(envText()).toContain("REGENT_SLACK_TOKEN=xoxb-piped");
    expect(envText()).toContain("REGENT_GATEWAY_PLATFORM=slack");
  });

  // The removal, proven: each of these used to save a token. A leak that has
  // already reached shell history cannot be un-leaked by a warning, so the
  // command must refuse — and refuse before touching .env.
  test("every command-line token form is refused and writes nothing", () => {
    for (const args of [
      ["setup", "telegram", "8910084620:AAEDy-in-history"], // documented form
      ["setup", "--token", "8910084620:AAEDy-in-history"], // documented flag
      ["setup", "8910084620:AAEDy-in-history"], // bare back-compat form
      ["setup", "telegram", "8910084620:AAEDy-in-history", "--token-stdin"], // both
    ]) {
      freshHome();
      expect(gatewayCommand("", args, () => "unused\n")).toBe(2);
      expect(hasEnv()).toBe(false);
      rmSync(home, { recursive: true, force: true });
    }
  });

  test("a missing --token-stdin is a usage error, not a silent empty token", () => {
    freshHome();
    expect(gatewayCommand("", ["setup", "telegram"], () => "unused\n")).toBe(1);
    expect(hasEnv()).toBe(false);
  });

  test("an empty pipe and an injected second assignment both fail closed", () => {
    freshHome();
    expect(gatewayCommand("", ["setup", "telegram", "--token-stdin"], () => "\n")).toBe(1);
    expect(hasEnv()).toBe(false);
    expect(
      gatewayCommand(
        "",
        ["setup", "telegram", "--token-stdin"],
        () => "tok\nREGENT_API_KEY=stolen\n",
      ),
    ).toBe(1);
    expect(hasEnv()).toBe(false);
  });
});
