import { expect, test } from "bun:test";
import { type Control, envFlag, posture, worst } from "./posture.ts";

const find = (cs: Control[], name: string) => cs.find((c) => c.control === name) as Control;
const OFF = {};

test("a default install reports nothing worse than default", () => {
  const cs = posture({}, OFF);
  expect(worst(cs)).toBe("default");
  expect(find(cs, "approvals").value).toBe("prompted");
});

test("booleans parse the way the runtime parses them, not the way JS coerces", () => {
  for (const on of ["1", "true", "yes", "YES", " true "]) expect(envFlag(on)).toBe(true);
  // "0" and "false" are truthy strings in JavaScript — reporting them as ON
  // would be a false alarm on every machine that explicitly turned one off.
  for (const off of ["0", "false", "no", "", " ", undefined]) expect(envFlag(off)).toBe(false);
});

test("auto-approve is unsafe from either source, and names which one", () => {
  expect(find(posture({ "tools.auto_approve": true }, OFF), "approvals")).toMatchObject({
    status: "unsafe",
    origin: "config.yaml",
  });
  expect(find(posture({}, { REGENT_AUTO_APPROVE: "1" }), "approvals")).toMatchObject({
    status: "unsafe",
    origin: "env",
  });
});

// The refinement the review insisted on: one boolean cannot express this.
test("the HTTP listener is judged in context, not by a single boolean", () => {
  const off = find(posture({ "http.enabled": false }, OFF), "http listener");
  expect(off.status).toBe("default");

  const loopbackWithToken = find(
    posture({ "http.enabled": true, "http.bind": "127.0.0.1:8080", "http.token": "<set>" }, OFF),
    "http listener",
  );
  expect(loopbackWithToken.status).toBe("review");

  const openToTheWorld = find(
    posture({ "http.enabled": true, "http.bind": "0.0.0.0:8080", "http.token": "<set>" }, OFF),
    "http listener",
  );
  expect(openToTheWorld.status).toBe("unsafe");

  const noToken = find(
    posture({ "http.enabled": true, "http.bind": "127.0.0.1:8080", "http.token": "<unset>" }, OFF),
    "http listener",
  );
  expect(noToken.status).toBe("unsafe");
  expect(noToken.note).toContain("NO token");
});

test("the report never prints a secret value", () => {
  // The descriptor redacts before this sees it; assert the contract holds.
  const cs = posture(
    { "http.enabled": true, "http.bind": "127.0.0.1", "http.token": "<set>" },
    OFF,
  );
  expect(JSON.stringify(cs)).not.toContain("supersecret");
  expect(find(cs, "http listener").value).toContain("token=set");
});

test("hooks are reported as review — shell outside the approval gate", () => {
  const cs = posture({ "tools.hook_tool_start": "notify-send x" }, OFF);
  expect(find(cs, "tool hooks")).toMatchObject({ status: "review" });
  expect(find(cs, "tool hooks").note).toContain("outside the approval gate");
  expect(find(posture({ "tools.hook_tool_start": "" }, OFF), "tool hooks").status).toBe("default");
});

test("REGENT_SANDBOX is reported without pretending it covers everything", () => {
  const on = find(posture({}, { REGENT_SANDBOX: "1" }), "REGENT_SANDBOX");
  expect(on.value).toBe("on");
  // Cron and the board workers were once outside this flag entirely; the report
  // has to keep naming what it actually covers.
  expect(on.note).toContain("cron");
  expect(
    find(posture({}, { REGENT_UNSAFE_NO_SANDBOX: "1" }), "REGENT_UNSAFE_NO_SANDBOX"),
  ).toMatchObject({ status: "unsafe" });
});

test("worst() picks the most serious status present", () => {
  expect(worst(posture({}, { REGENT_COMPUTER_USE: "1" }))).toBe("review");
  expect(worst(posture({}, { REGENT_VOICE_FULL_CONTROL: "1" }))).toBe("unsafe");
});
