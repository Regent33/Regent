import { afterEach, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  coerce,
  readConfig,
  setDotted,
  unsetDotted,
  withConfigLock,
  writeConfigAtomically,
} from "./configFile.ts";

const homes: string[] = [];
function home(contents?: string): string {
  const h = mkdtempSync(join(tmpdir(), "regent-cfg-"));
  homes.push(h);
  if (contents !== undefined) writeFileSync(join(h, "config.yaml"), contents);
  return h;
}
afterEach(() => {
  for (const h of homes.splice(0)) rmSync(h, { recursive: true, force: true });
});

test("a missing file and an unparseable file are different answers", () => {
  expect(readConfig(home()).kind).toBe("missing");
  // The bug this exists for: `config set` treated both as "start fresh" and
  // wrote a new file over the one it could not read.
  expect(readConfig(home("model: {default: x\n  bad")).kind).toBe("malformed");
  expect(readConfig(home("- a\n- b\n")).kind).toBe("malformed"); // sequence, not mapping
  expect(readConfig(home("")).kind).toBe("ok"); // empty file is an empty config
  expect(readConfig(home("model:\n  default: x\n")).kind).toBe("ok");
});

test("a schema-invalid file still parses — that is what makes `unset` a repair", () => {
  const r = readConfig(home("_config_version: 2\nmodel:\n  defalut: typo\n"));
  expect(r.kind).toBe("ok");
  if (r.kind !== "ok") return;
  expect(unsetDotted(r.doc, "model.defalut")).toBe(true);
  expect(unsetDotted(r.doc, "model.defalut")).toBe(false); // already gone
  expect(unsetDotted(r.doc, "nothing.here")).toBe(false);
});

test("the atomic write preserves unrelated keys and leaves no temp file", () => {
  const h = home("_config_version: 2\ncron:\n  tick_interval_secs: 99\n");
  const r = readConfig(h);
  if (r.kind !== "ok") throw new Error("fixture should parse");
  setDotted(r.doc, "model.default", "claude-opus-5");
  writeConfigAtomically(h, r.doc);
  const after = readFileSync(join(h, "config.yaml"), "utf8");
  expect(after).toContain("tick_interval_secs: 99");
  expect(after).toContain("claude-opus-5");
  expect(after).toContain("_config_version: 2"); // never downgraded to 1
  expect(existsSync(join(h, `config.yaml.tmp.${process.pid}`))).toBe(false);
});

test("the lock is exclusive and is released afterwards", () => {
  const h = home("a: 1\n");
  const lock = join(h, "config.yaml.lock");
  withConfigLock(h, () => {
    expect(existsSync(lock)).toBe(true);
    // A second writer must not get in — it waits, then names the lock.
    expect(() => withConfigLock(h, () => 0, 100)).toThrow(/another regent process/);
  });
  expect(existsSync(lock)).toBe(false);
});

test("the lock is released even when the write throws", () => {
  const h = home("a: 1\n");
  expect(() =>
    withConfigLock(h, () => {
      throw new Error("boom");
    }),
  ).toThrow("boom");
  expect(existsSync(join(h, "config.yaml.lock"))).toBe(false);
});

test("dotted paths cannot walk into the JavaScript object graph", () => {
  const doc: Record<string, unknown> = {};
  for (const bad of ["__proto__.polluted", "a.constructor.x", "model..default", ".model"]) {
    expect(() => setDotted(doc, bad, "x")).toThrow(/invalid key/);
    expect(() => unsetDotted(doc, bad)).toThrow(/invalid key/);
  }
  expect(({} as Record<string, unknown>).polluted).toBeUndefined();
});

test("the lock is created even when the home does not exist yet", () => {
  const h = join(tmpdir(), `regent-cfg-new-${process.pid}`);
  homes.push(h);
  // First `config set` on a fresh profile: no ENOENT retry loop misreported as
  // "another process is writing".
  expect(withConfigLock(h, () => "ran", 100)).toBe("ran");
  expect(existsSync(h)).toBe(true);
});

test("setDotted replaces a scalar parent instead of throwing", () => {
  const doc: Record<string, unknown> = { model: "a string, not a section" };
  setDotted(doc, "model.default", "x");
  expect(doc.model).toEqual({ default: "x" });
});

test("coerce types booleans and numbers, leaves everything else alone", () => {
  expect(coerce("true")).toBe(true);
  expect(coerce("false")).toBe(false);
  expect(coerce("42")).toBe(42);
  expect(coerce("-1.5")).toBe(-1.5);
  expect(coerce("claude-opus-5")).toBe("claude-opus-5");
  expect(coerce("1.2.3")).toBe("1.2.3"); // a version is not a number
});

// Several config keys are LISTS — tools.deferred, tools.pinned,
// providers.<name>.models. Coercing an array to a string wrote the brackets as
// text: config.yaml came out holding `deferred: '["a","b"]'`, one string, and
// tool deferral silently stopped matching any tool. Reproduced against a real
// config before this fix, and the write reported success either way, which is
// what made it dangerous.
test("coerce parses a JSON array so list keys stay lists", () => {
  expect(coerce('["a","b"]')).toEqual(["a", "b"]);
  expect(coerce("[]")).toEqual([]);
  expect(coerce("[1, 2]")).toEqual([1, 2]);
});

test("coerce leaves anything that only looks bracketed as a string", () => {
  // Malformed JSON, and arrays of objects, must not throw or half-parse — a
  // value can legitimately contain brackets.
  expect(coerce('["a", ')).toBe('["a", ');
  expect(coerce("[not json]")).toBe("[not json]");
  expect(coerce('[{"a":1}]')).toBe('[{"a":1}]'); // no scalar list here
  expect(coerce("[")).toBe("[");
});
