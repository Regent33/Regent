// The CLI's remaining config responsibility is coercion. Everything that used
// to be tested here — malformed vs schema-invalid, locking, atomic writes,
// dotted-path safety — moved to Rust with the code, and is tested in
// `regent-deacon/src/infra/tests/config_offline.rs`.
import { expect, test } from "bun:test";
import { coerce } from "./configFile.ts";

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
