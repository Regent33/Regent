import { describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  type CommandResult,
  type CommandRunner,
  lockDownFile,
  lockDownWindowsFile,
} from "./lockdown.ts";

describe("Windows secret-file ACL", () => {
  test("grants the actual process principal and never consults USERNAME", () => {
    const calls: Array<readonly [string, readonly string[]]> = [];
    const run: CommandRunner = (command, args): CommandResult => {
      calls.push([command, args]);
      return command === "whoami"
        ? { status: 0, stdout: "machine\\service-user\r\n" }
        : { status: 0, stdout: "" };
    };

    lockDownWindowsFile("C:\\Regent\\.env.tmp", run);

    expect(calls).toEqual([
      ["whoami", []],
      ["icacls", ["C:\\Regent\\.env.tmp", "/inheritance:r", "/grant:r", "machine\\service-user:F"]],
    ]);
  });

  test("fails closed when identity discovery or the ACL grant fails", () => {
    expect(() =>
      lockDownWindowsFile("secret", () => ({ status: 1, stdout: "", stderr: "no identity" })),
    ).toThrow("whoami");

    const grantFails: CommandRunner = (command) =>
      command === "whoami"
        ? { status: 0, stdout: "machine\\user" }
        : { status: 5, stderr: "access denied" };
    expect(() => lockDownWindowsFile("secret", grantFails)).toThrow("access denied");
  });

  test("the real Windows ACL is owner-only before secret bytes are written", () => {
    if (process.platform !== "win32") return;
    const home = mkdtempSync(join(tmpdir(), "regent-acl-"));
    const path = join(home, ".env.tmp");
    try {
      writeFileSync(path, "", { flag: "wx", mode: 0o600 });
      lockDownFile(path);
      writeFileSync(path, "SAFE_API_KEY=secret\n");

      const who = spawnSync("whoami", [], { encoding: "utf8", windowsHide: true });
      const acl = spawnSync("icacls", [path], { encoding: "utf8", windowsHide: true });
      expect(who.status).toBe(0);
      expect(acl.status).toBe(0);
      expect(acl.stdout.toLowerCase()).toContain(who.stdout.trim().toLowerCase());
      expect(acl.stdout).not.toContain("(I)");
    } finally {
      rmSync(home, { recursive: true, force: true });
    }
  });
});
