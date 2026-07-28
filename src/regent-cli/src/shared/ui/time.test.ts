import { expect, test } from "bun:test";
import { fmtTime } from "./time.ts";

test("renders an epoch in local time, not UTC", () => {
  const epoch = 1_774_000_000; // some fixed instant
  const d = new Date(epoch * 1000);
  const expected = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(
    d.getDate(),
  ).padStart(
    2,
    "0",
  )} ${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  expect(fmtTime(epoch)).toBe(expected);
  // The bug this replaced: the ISO/UTC form differs wherever the offset is not
  // zero, which is where the eight-hour discrepancy came from.
  if (d.getTimezoneOffset() !== 0) {
    expect(fmtTime(epoch)).not.toBe(d.toISOString().slice(0, 16).replace("T", " "));
  }
});

test("shape is fixed-width so table columns line up", () => {
  expect(fmtTime(0)).toHaveLength(16);
  expect(fmtTime(1_774_000_000)).toHaveLength(16);
});

test("a nonsense epoch renders as a dash rather than 'Invalid Date'", () => {
  expect(fmtTime(Number.NaN)).toBe("-");
  expect(fmtTime(8.64e15)).toBe("-");
});
