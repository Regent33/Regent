import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { redirectedHome } from "./locate.ts";

const dirs: string[] = [];
const freshDef = () => {
  const d = mkdtempSync(join(tmpdir(), "regent-home-"));
  dirs.push(d);
  return d;
};
afterEach(() => {
  for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
});

describe("redirectedHome (~/.regent/.home data-dir pointer)", () => {
  test("no pointer file → the default", () => {
    const def = freshDef();
    expect(redirectedHome(def)).toBe(def);
  });

  test("pointer file redirects to the chosen directory", () => {
    const def = freshDef();
    writeFileSync(join(def, ".home"), "D:\\regent-data\n");
    expect(redirectedHome(def)).toBe("D:\\regent-data");
  });

  test("empty or self-pointing file → the default (no loops)", () => {
    const def = freshDef();
    writeFileSync(join(def, ".home"), "  \n");
    expect(redirectedHome(def)).toBe(def);
    writeFileSync(join(def, ".home"), `${def}\n`);
    expect(redirectedHome(def)).toBe(def);
  });
});
