import { describe, expect, test } from "bun:test";
import { RELEASES_URL, parseUpdateStatus, updateNotice } from "./notice.ts";

describe("parseUpdateStatus", () => {
  test("reads the minimal Phase-0 shape and ignores extra fields", () => {
    const s = parseUpdateStatus({
      current: "0.1.1",
      latest: "0.2.0",
      available: true,
      checked_at: 123,
      source: "network",
      note: "whatever",
    });
    expect(s).not.toBeNull();
    expect(s?.available).toBe(true);
    expect(s?.latest).toBe("0.2.0");
    expect(s?.current).toBe("0.1.1");
  });

  test("a null / non-object / mis-typed body is not a status", () => {
    for (const bad of [null, undefined, 42, "x", [], { available: "yes" }, {}]) {
      expect(parseUpdateStatus(bad)).toBeNull();
    }
  });

  test("an absent or empty `latest` normalizes to null", () => {
    expect(parseUpdateStatus({ current: "0.1.1", available: false })?.latest).toBeNull();
    expect(parseUpdateStatus({ latest: "", available: false })?.latest).toBeNull();
  });
});

describe("updateNotice", () => {
  test("stays silent unless an upgrade is actually available", () => {
    expect(updateNotice(null, "0.1.1")).toBeNull();
    expect(updateNotice({ current: "0.1.1", latest: null, available: false }, "0.1.1")).toBeNull();
    // `available` true but no known latest → still nothing actionable to say.
    expect(updateNotice({ current: "0.1.1", latest: null, available: true }, "0.1.1")).toBeNull();
  });

  test("when available, names the CLI's own version and the fixed release URL", () => {
    const line = updateNotice({ current: "0.1.1", latest: "0.2.0", available: true }, "0.1.1");
    expect(line).not.toBeNull();
    // Contract, not prose: it carries the offered version, the CLI's own
    // version, and the official URL — never a remote/manifest URL.
    expect(line).toContain("0.2.0");
    expect(line).toContain("0.1.1");
    expect(line).toContain(RELEASES_URL);
  });

  test("does not assert the deacon and CLI are the same component", () => {
    // Deacon (`current`) and CLI version differ; both appear, neither is
    // presented as equal to the other.
    const line = updateNotice({ current: "0.1.0", latest: "0.2.0", available: true }, "0.1.1");
    expect(line).toContain("0.1.1"); // the CLI's own version, as passed in
    expect(line).toContain("0.2.0"); // the offered release
  });
});
