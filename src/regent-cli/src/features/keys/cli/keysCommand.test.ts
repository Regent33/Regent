import { afterEach, describe, expect, test } from "bun:test";
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { keysCommand } from "./keysCommand.ts";

let home: string | undefined;
const priorHome = process.env.REGENT_HOME;
const cliRoot = join(import.meta.dir, "../../../..");

function setInChild(home: string, key: string, value: string): Promise<number | null> {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, ["run", "src/main.tsx", "keys", "set", key, "--stdin"], {
      cwd: cliRoot,
      env: { ...process.env, REGENT_HOME: home, NO_COLOR: "1" },
      stdio: ["pipe", "pipe", "pipe"],
      windowsHide: true,
    });
    let error = "";
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk) => {
      error += chunk;
    });
    child.on("error", reject);
    child.on("exit", (code) => {
      if (code !== 0) reject(new Error(`${key} exited ${code}: ${error}`));
      else resolve(code);
    });
    child.stdin.end(`${value}\n`);
  });
}

afterEach(() => {
  if (home) rmSync(home, { recursive: true, force: true });
  home = undefined;
  if (priorHome === undefined) delete process.env.REGENT_HOME;
  else process.env.REGENT_HOME = priorHome;
});

describe("regent keys media providers", () => {
  test("sets, replaces, lists, and removes image/video provider keys", () => {
    home = mkdtempSync(join(tmpdir(), "regent-keys-"));
    process.env.REGENT_HOME = home;

    expect(
      keysCommand("", ["set", "STABILITY_API_KEY", "--stdin"], () => "image-secret-1234\n"),
    ).toBe(0);
    expect(
      keysCommand("", ["set", "RUNWAYML_API_SECRET", "--stdin"], () => "video-secret-5678\n"),
    ).toBe(0);
    expect(
      keysCommand("", ["set", "RUNWAYML_API_SECRET", "--stdin"], () => "replacement-9012\n"),
    ).toBe(0);
    expect(keysCommand("", ["list"])).toBe(0);

    const saved = readFileSync(join(home, ".env"), "utf8");
    expect(saved).toContain("STABILITY_API_KEY=image-secret-1234");
    expect(saved).toContain("RUNWAYML_API_SECRET=replacement-9012");
    expect(saved).not.toContain("video-secret-5678");

    expect(keysCommand("", ["rm", "STABILITY_API_KEY"])).toBe(0);
    expect(readFileSync(join(home, ".env"), "utf8")).not.toContain("STABILITY_API_KEY=");
  });

  test("rejects command-line secrets and multiline env injection without changing the file", () => {
    home = mkdtempSync(join(tmpdir(), "regent-keys-"));
    process.env.REGENT_HOME = home;
    expect(keysCommand("", ["set", "STABILITY_API_KEY", "--stdin"], () => "safe-value\n")).toBe(0);
    const before = readFileSync(join(home, ".env"), "utf8");

    expect(keysCommand("", ["set", "RUNWAYML_API_SECRET", "visible-in-history"])).toBe(1);
    expect(
      keysCommand(
        "",
        ["set", "RUNWAYML_API_SECRET", "--stdin"],
        () => "video-value\nREGENT_API_KEY=stolen",
      ),
    ).toBe(1);
    expect(keysCommand("", ["set", "BAD\nREGENT_API_KEY", "--stdin"], () => "stolen")).toBe(1);
    expect(keysCommand("", ["set", "RUNWAYML_API_SECRET", "--stdin"], () => "bad\0value")).toBe(1);
    expect(keysCommand("", ["set", "REGENT_AUTO_APPROVE", "--stdin"], () => "1\n")).toBe(1);
    expect(keysCommand("", ["rm", "REGENT_AUTO_APPROVE"])).toBe(1);

    expect(readFileSync(join(home, ".env"), "utf8")).toBe(before);
  });

  test("allows catalogued model and browser keys without opening a runtime-variable escape hatch", () => {
    home = mkdtempSync(join(tmpdir(), "regent-keys-"));
    process.env.REGENT_HOME = home;

    expect(keysCommand("", ["set", "ANTHROPIC_API_KEY", "--stdin"], () => "model-key\n")).toBe(0);
    expect(
      keysCommand(
        "",
        ["set", "REGENT_BROWSER_MCP_URL", "--stdin"],
        () => "http://127.0.0.1:8931/sse\n",
      ),
    ).toBe(0);
    const saved = readFileSync(join(home, ".env"), "utf8");
    expect(saved).toContain("ANTHROPIC_API_KEY=model-key");
    expect(saved).toContain("REGENT_BROWSER_MCP_URL=http://127.0.0.1:8931/sse");
    expect(saved).not.toContain("REGENT_AUTO_APPROVE=");
  });

  test("normalizes duplicates, removes every assignment, and manages numbered slots", () => {
    home = mkdtempSync(join(tmpdir(), "regent-keys-"));
    process.env.REGENT_HOME = home;
    const path = join(home, ".env");
    writeFileSync(path, "TAVILY_API_KEY=old-one\nOTHER=value\n TAVILY_API_KEY=old-two\n");

    expect(keysCommand("", ["set", "TAVILY_API_KEY", "--stdin"], () => "canonical\n")).toBe(0);
    expect(readFileSync(path, "utf8").match(/TAVILY_API_KEY=/g)?.length).toBe(1);
    expect(keysCommand("", ["rm", "TAVILY_API_KEY"])).toBe(0);
    expect(readFileSync(path, "utf8")).not.toContain("TAVILY_API_KEY=");

    expect(keysCommand("", ["set", "OPENROUTER_API_KEY_2", "--stdin"], () => "backup-key\n")).toBe(
      0,
    );
    expect(readFileSync(path, "utf8")).toContain("OPENROUTER_API_KEY_2=backup-key");
    expect(keysCommand("", ["rm", "OPENROUTER_API_KEY_2"])).toBe(0);
    expect(readFileSync(path, "utf8")).not.toContain("OPENROUTER_API_KEY_2=");
    expect(keysCommand("", ["set", "OPENROUTER_API_KEY_9", "--stdin"], () => "bad\n")).toBe(1);
  });

  test("five real CLI processes preserve all five concurrent writes", async () => {
    home = mkdtempSync(join(tmpdir(), "regent-keys-concurrent-"));
    const assignments = [
      ["STABILITY_API_KEY", "image-one"],
      ["RUNWAYML_API_SECRET", "video-two"],
      ["FAL_KEY", "image-three"],
      ["TAVUS_API_KEY", "video-four"],
      ["REPLICATE_API_TOKEN", "image-five"],
    ] as const;

    await Promise.all(assignments.map(([key, value]) => setInChild(home ?? "", key, value)));
    const saved = readFileSync(join(home, ".env"), "utf8");
    for (const [key, value] of assignments) {
      expect(saved).toContain(`${key}=${value}`);
    }
  }, 30_000);
  // A spaced assignment is LIVE: the deacon's loader splits on the first `=`
  // and trims the name, and `keys remove` deletes such a line. Only the list
  // lookup demanded that `=` touch the key, so a credential that was loaded and
  // authenticating printed as "not set" here while the Desktop page - which
  // asks the deacon - showed it set, for the same bytes on disk.
  test("a spaced assignment lists as set, matching the loader and the app", () => {
    home = mkdtempSync(join(tmpdir(), "regent-keys-spaced-"));
    process.env.REGENT_HOME = home;
    writeFileSync(join(home, ".env"), "FAL_KEY =fal-live-secret\n");
    // `out()` writes straight to process.stdout, so console.log is not the
    // seam — capturing the wrong one made this assert on an empty array.
    const lines: string[] = [];
    const originalWrite = process.stdout.write.bind(process.stdout);
    (process.stdout as { write: unknown }).write = (chunk: unknown): boolean => {
      lines.push(String(chunk));
      return true;
    };
    try {
      // First arg is a PROFILE, not a home path - `regentHome("")` reads
      // REGENT_HOME, which the line above set.
      expect(keysCommand("", ["list"], () => "")).toBe(0);
    } finally {
      (process.stdout as { write: unknown }).write = originalWrite;
    }
    const row = lines.find((l) => l.includes("FAL_KEY"));
    expect(row).toBeDefined();
    expect(row).toContain("set");
    expect(row).not.toContain("not set");
    // ...and the secret itself is never printed.
    expect(lines.join("\n")).not.toContain("fal-live-secret");
  });
});
