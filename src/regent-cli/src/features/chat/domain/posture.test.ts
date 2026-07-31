import { expect, test } from "bun:test";
import { unattendedMarkers } from "./posture.ts";

const OFF = {}; // no env flags set

test("a default session shows nothing — the marker has to keep its meaning", () => {
  expect(unattendedMarkers({ tools: { auto_approve: false } }, OFF)).toEqual([]);
  expect(unattendedMarkers(null, OFF)).toEqual([]);
  expect(unattendedMarkers({}, OFF)).toEqual([]);
});

test("auto-approve is reported from either the config or the env var", () => {
  expect(unattendedMarkers({ tools: { auto_approve: true } }, OFF)).toEqual(["auto-approve"]);
  expect(unattendedMarkers(null, { REGENT_AUTO_APPROVE: "1" })).toEqual(["auto-approve"]);
  // Reported once, not twice, when both are on.
  expect(
    unattendedMarkers({ tools: { auto_approve: true } }, { REGENT_AUTO_APPROVE: "1" }),
  ).toEqual(["auto-approve"]);
});

test("env flags parse the same forms the Rust side accepts, and no others", () => {
  for (const on of ["1", "true", "yes", "TRUE", " 1 "]) {
    expect(unattendedMarkers(null, { REGENT_AUTO_APPROVE: on })).toEqual(["auto-approve"]);
  }
  // An unset or falsey var must not raise a warning nobody can act on.
  for (const off of ["", "0", "false", "no", undefined]) {
    expect(unattendedMarkers(null, { REGENT_AUTO_APPROVE: off })).toEqual([]);
  }
});

test("disabling the sandbox and voice full-control are reported", () => {
  expect(unattendedMarkers(null, { REGENT_UNSAFE_NO_SANDBOX: "1" })).toEqual(["sandbox off"]);
  expect(unattendedMarkers(null, { REGENT_VOICE_FULL_CONTROL: "1" })).toEqual([
    "voice full-control",
  ]);
});

test("configured tool hooks are reported — they run shell outside the approval gate", () => {
  expect(unattendedMarkers({ tools: { hook_tool_start: "notify-send x" } }, OFF)).toEqual([
    "tool hooks",
  ]);
  // The default is an empty string, which is not a hook.
  expect(unattendedMarkers({ tools: { hook_tool_start: "" } }, OFF)).toEqual([]);
  expect(unattendedMarkers({ tools: { hook_tool_start: "   " } }, OFF)).toEqual([]);
});

test("several at once read as a list", () => {
  expect(
    unattendedMarkers({ tools: { auto_approve: true } }, { REGENT_UNSAFE_NO_SANDBOX: "1" }),
  ).toEqual(["auto-approve", "sandbox off"]);
});
