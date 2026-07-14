import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { markSetupDone, needsOnboarding } from "./firstRun.ts";

const homes: string[] = [];
const freshHome = () => {
  const h = mkdtempSync(join(tmpdir(), "regent-firstrun-"));
  homes.push(h);
  return h;
};
afterEach(() => {
  for (const h of homes.splice(0)) rmSync(h, { recursive: true, force: true });
});

describe("needsOnboarding", () => {
  test("fresh empty home → wizard", () => {
    expect(needsOnboarding(freshHome())).toBe(true);
  });

  test("deacon-seeded config.yaml alone does NOT skip the wizard (the 2026-07-14 bug)", () => {
    const home = freshHome();
    writeFileSync(join(home, "config.yaml"), "_config_version: 1\n");
    expect(needsOnboarding(home)).toBe(true);
  });

  test("completed wizard (marker) → no wizard, even with no .env (ollama path)", () => {
    const home = freshHome();
    markSetupDone(home);
    expect(needsOnboarding(home)).toBe(false);
  });

  test("pre-marker install with credentials (.env) → no wizard", () => {
    const home = freshHome();
    writeFileSync(join(home, ".env"), "REGENT_API_KEY=sk-test\n");
    expect(needsOnboarding(home)).toBe(false);
  });
});
