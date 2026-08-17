import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { withDotenvLock } from "./dotenvLock.ts";

describe("shared dotenv lock", () => {
  test("is released after success and after an exception", () => {
    const home = mkdtempSync(join(tmpdir(), "regent-dotenv-lock-"));
    const path = join(home, ".env");
    try {
      expect(withDotenvLock(path, () => 42)).toBe(42);
      expect(existsSync(`${path}.lock`)).toBe(false);

      expect(() =>
        withDotenvLock(path, () => {
          throw new Error("write failed");
        }),
      ).toThrow("write failed");
      expect(existsSync(`${path}.lock`)).toBe(false);
    } finally {
      rmSync(home, { recursive: true, force: true });
    }
  });
});
