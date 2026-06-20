import { expect, test } from "bun:test";
import { err, failure, isErr, isOk, ok, unwrapOr } from "./result.ts";

test("ok constructs and narrows to its value", () => {
  const r = ok(5);
  expect(r.ok).toBe(true);
  expect(isOk(r)).toBe(true);
  if (isOk(r)) expect(r.value).toBe(5);
});

test("err constructs and narrows to its failure", () => {
  const r = err(failure("rpc", "boom"));
  expect(isErr(r)).toBe(true);
  if (isErr(r)) expect(r.error.message).toBe("boom");
});

test("unwrapOr returns the value on ok and the fallback on err", () => {
  expect(unwrapOr(ok(1), 9)).toBe(1);
  expect(unwrapOr(err(failure("k", "y")), 9)).toBe(9);
});

test("failure omits cause when undefined and keeps it when given", () => {
  expect("cause" in failure("k", "m")).toBe(false);
  expect(failure("k", "m", "c").cause).toBe("c");
});
