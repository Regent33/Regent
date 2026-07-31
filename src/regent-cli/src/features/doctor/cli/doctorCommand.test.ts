// The verdict rule is the whole compatibility story of `doctor`: the default
// exit code must not change, and `--strict` is what CI opts into.
import { expect, test } from "bun:test";
import { type Check, doctorVerdict } from "./doctorCommand.ts";

const c = (severity: Check["severity"]): Check => ({ check: "x", severity, detail: "" });

test("all ok passes in both modes", () => {
  expect(doctorVerdict([c("ok"), c("ok")], false)).toEqual({ status: "ok", code: 0 });
  expect(doctorVerdict([c("ok")], true)).toEqual({ status: "ok", code: 0 });
});

test("a posture warning does NOT fail by default — that is the point of --strict", () => {
  expect(doctorVerdict([c("ok"), c("warn")], false)).toEqual({ status: "warn", code: 0 });
  expect(doctorVerdict([c("ok"), c("warn")], true)).toEqual({ status: "warn", code: 1 });
});

test("a failed operational check fails in both modes", () => {
  expect(doctorVerdict([c("warn"), c("fail")], false)).toEqual({ status: "fail", code: 1 });
  expect(doctorVerdict([c("fail")], true)).toEqual({ status: "fail", code: 1 });
});

test("no checks at all is not a failure", () => {
  expect(doctorVerdict([], true)).toEqual({ status: "ok", code: 0 });
});
