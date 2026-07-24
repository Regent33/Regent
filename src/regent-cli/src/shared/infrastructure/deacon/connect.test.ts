import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join, resolve } from "node:path";
import type { IRpcClient, RpcFailure } from "@shared/kernel/contracts.ts";
import { type Result, err, ok } from "@shared/kernel/result.ts";
import { connectHealthyDeacon } from "./connect.ts";
import { binaryCandidates } from "./locate.ts";
import type { SpawnedDeacon } from "./spawn.ts";

const dirs: string[] = [];
const freshDir = (): string => {
  const d = mkdtempSync(join(tmpdir(), "regent-cand-"));
  dirs.push(d);
  return d;
};
afterEach(() => {
  for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
});

const EXE = process.platform === "win32" ? ".exe" : "";
// A base name that exists nowhere in target/ or PATH, so binaryCandidates only
// returns the temp files this test plants — keeps ordering assertions
// deterministic regardless of the repo/target/PATH the suite runs in.
const BASE = "regent-cand-probe";
const ENV = "REGENT_CAND_PROBE_PATH";

function plant(dir: string): string {
  const p = join(dir, BASE + EXE);
  writeFileSync(p, "not a real binary");
  return p;
}

describe("binaryCandidates ordering + dedup", () => {
  const savedEnv = process.env[ENV];
  const savedPath = process.env.PATH;
  afterEach(() => {
    if (savedEnv === undefined) delete process.env[ENV];
    else process.env[ENV] = savedEnv;
    process.env.PATH = savedPath;
  });

  test("override is first, PATH entries follow, only existing files appear", () => {
    const overrideDir = freshDir();
    const pathDir = freshDir();
    const override = plant(overrideDir);
    const onPath = plant(pathDir);
    process.env[ENV] = override;
    process.env.PATH = [pathDir, freshDir() /* empty, no binary */].join(delimiter);

    const got = binaryCandidates(BASE, ENV);
    expect(got[0]).toBe(resolve(override));
    expect(got).toContain(resolve(onPath));
    // The empty PATH dir contributes nothing.
    expect(got.length).toBe(2);
  });

  test("the same file reached two ways is de-duplicated", () => {
    const dir = freshDir();
    const bin = plant(dir);
    process.env[ENV] = bin;
    process.env.PATH = dir; // override AND PATH point at the same file

    const got = binaryCandidates(BASE, ENV);
    expect(got).toEqual([resolve(bin)]);
  });

  test("a non-existent override is dropped from the list", () => {
    const pathDir = freshDir();
    const onPath = plant(pathDir);
    process.env[ENV] = join(freshDir(), `missing${EXE}`);
    process.env.PATH = pathDir;

    const got = binaryCandidates(BASE, ENV);
    expect(got).toEqual([resolve(onPath)]);
  });
});

// A fake child: `call("health")` resolves ok/err on demand; `close()` is tracked
// so the fallback loop's teardown of a failed candidate is observable.
function fakeChild(
  healthOk: boolean,
  stderr = "",
): {
  spawned: SpawnedDeacon;
  closed: () => boolean;
} {
  let closed = false;
  const client: IRpcClient = {
    call: <T = unknown>(): Promise<Result<T, RpcFailure>> =>
      Promise.resolve(
        healthOk ? ok(undefined as T) : err({ kind: "rpc", message: "deacon stream closed" }),
      ),
    onNotification: () => () => {},
    close: () => {
      closed = true;
      return Promise.resolve();
    },
  };
  return { spawned: { client, earlyStderr: () => stderr }, closed: () => closed };
}

describe("connectHealthyDeacon fallback loop", () => {
  const home = () => freshDir();

  test("falls through a boots-then-dies candidate to the next healthy one", async () => {
    const dead = fakeChild(false, "fatal: yaml: model: unknown field `review`");
    const alive = fakeChild(true);
    const table: Record<string, SpawnedDeacon> = {
      "/stale": dead.spawned,
      "/fresh": alive.spawned,
    };

    const res = await connectHealthyDeacon(home(), {
      candidates: ["/stale", "/fresh"],
      spawn: (p) => ok(table[p] as SpawnedDeacon),
    });

    expect(res.ok).toBe(true);
    if (res.ok) expect(res.value.path).toBe("/fresh");
    expect(dead.closed()).toBe(true); // the dead candidate was torn down
    expect(alive.closed()).toBe(false); // the winner stays open
  });

  test("when every candidate fails, the error lists each path and its reason", async () => {
    const a = fakeChild(false, "fatal: yaml: model: unknown field `review`");
    const table: Record<string, SpawnedDeacon> = { "/a": a.spawned };

    const res = await connectHealthyDeacon(home(), {
      candidates: ["/a", "/b"],
      spawn: (p) =>
        table[p]
          ? ok(table[p] as SpawnedDeacon)
          : err({ kind: "deacon-spawn", message: "spawn /b: ENOENT" }),
    });

    expect(res.ok).toBe(false);
    if (!res.ok) {
      expect(res.error.message).toContain("/a");
      expect(res.error.message).toContain("unknown field `review`"); // captured stderr
      expect(res.error.message).toContain("/b");
      expect(res.error.message).toContain("ENOENT"); // spawn-failure reason
    }
  });

  test("no candidates → a build/install hint, not an opaque error", async () => {
    const res = await connectHealthyDeacon(home(), { candidates: [] });
    expect(res.ok).toBe(false);
    if (!res.ok) expect(res.error.message).toContain("regent-deacon not found");
  });
});
