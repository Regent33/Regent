import { afterEach, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { readDotenvLines, updateDotenv } from "./dotenvFile.ts";

let dir: string | undefined;

afterEach(() => {
  if (dir !== undefined) rmSync(dir, { recursive: true, force: true });
  dir = undefined;
});

describe("dotenv reads fail closed", () => {
  test("a missing .env is empty, so the first save on a fresh install works", () => {
    dir = mkdtempSync(join(tmpdir(), "regent-dotenv-fresh-"));
    const path = join(dir, ".env");
    expect(readDotenvLines(path)).toEqual([]);
    updateDotenv(path, { STABILITY_API_KEY: "sk-first" }, () => undefined);
    expect(readFileSync(path, "utf8")).toContain("STABILITY_API_KEY=sk-first");
  });

  test("an unreadable .env throws instead of publishing a one-line file", () => {
    dir = mkdtempSync(join(tmpdir(), "regent-dotenv-blocked-"));
    // A directory where the file belongs: readFileSync fails with something
    // that is NOT ENOENT, which is the class that used to become `[]`.
    const path = join(dir, ".env");
    mkdirSync(path);

    expect(() => readDotenvLines(path)).toThrow(/cannot read/);
    // The write path is read-modify-publish: with the error swallowed, `kept`
    // became just the key being written and the atomic replace destroyed every
    // other credential while reporting success.
    expect(() => updateDotenv(path, { STABILITY_API_KEY: "sk-new" }, () => undefined)).toThrow();
  });

  test("an existing file survives a write of one unrelated key", () => {
    dir = mkdtempSync(join(tmpdir(), "regent-dotenv-keep-"));
    const path = join(dir, ".env");
    writeFileSync(path, "ANTHROPIC_API_KEY=sk-keep-me\nFAL_KEY=fal-keep-me\n");
    updateDotenv(path, { STABILITY_API_KEY: "sk-added" }, () => undefined);
    const saved = readFileSync(path, "utf8");
    expect(saved).toContain("ANTHROPIC_API_KEY=sk-keep-me");
    expect(saved).toContain("FAL_KEY=fal-keep-me");
    expect(saved).toContain("STABILITY_API_KEY=sk-added");
  });
});
