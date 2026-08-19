import { describe, expect, test } from "bun:test";
import { CLI_VERSION } from "@app/cli/help.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { ok } from "@shared/kernel/result.ts";
import { updateCommand } from "./updateCommand.ts";

// Derived from CLI_VERSION, never written out. The command decides "mixed
// installation" by comparing the deacon's version against the CLI's OWN, so a
// literal like "0.1.2" here stops meaning "matching install" the moment the
// CLI moves past it — at 0.1.3 three of these tests flipped to the opposite
// case and failed, having tested nothing of what their names claimed.
const NEWER_THAN_CLI = `${Number(CLI_VERSION.split(".")[0]) + 1}.0.0`;
const OLDER_THAN_CLI = "0.0.1"; // predates every release, and versions only go up

const client = (value: unknown): IRpcClient =>
  ({
    call: async () => ok(value),
    close: async () => undefined,
    onNotification: () => () => undefined,
  }) as IRpcClient;

const capture = () => {
  const lines: string[] = [];
  const errors: string[] = [];
  return {
    lines,
    errors,
    output: {
      line: (message: string) => lines.push(message),
      error: (message: string) => errors.push(message),
    },
  };
};

describe("regent update", () => {
  test("reports a cached current release without mutating anything", async () => {
    expect(
      await updateCommand(
        client({
          current: CLI_VERSION,
          latest: CLI_VERSION,
          available: false,
          checked_at: 1_786_900_000,
          source: "cache",
        }),
        [],
      ),
    ).toBe(0);
  });

  test("reports an available release and accepts the check alias", async () => {
    expect(
      await updateCommand(
        client({
          current: CLI_VERSION,
          latest: NEWER_THAN_CLI,
          available: true,
          checked_at: 1_786_900_000,
          source: "network",
        }),
        ["--check"],
      ),
    ).toBe(0);
  });

  test("never checked and disabled are failures, not false up-to-date", async () => {
    expect(
      await updateCommand(
        client({ current: CLI_VERSION, latest: null, available: false, source: "never" }),
        [],
      ),
    ).toBe(1);
    expect(
      await updateCommand(
        client({ current: CLI_VERSION, latest: null, available: false, source: "disabled" }),
        [],
      ),
    ).toBe(1);
  });

  test("both mixed-install directions fail instead of claiming up to date", async () => {
    for (const status of [
      { current: NEWER_THAN_CLI, latest: NEWER_THAN_CLI, available: false, source: "cache" },
      { current: OLDER_THAN_CLI, latest: CLI_VERSION, available: true, source: "cache" },
      { current: "", latest: CLI_VERSION, available: false, source: "cache" },
    ]) {
      const seen = capture();
      expect(await updateCommand(client(status), [], seen.output)).toBe(1);
      expect(seen.lines.join("\n")).toContain("mixed installation");
      expect(seen.lines.join("\n")).not.toContain("up to date");
      expect(seen.errors.join("\n")).toContain("different releases");
    }
  });

  test("invalid usage fails before RPC", async () => {
    let called = false;
    const fake = client({});
    fake.call = async <T = unknown>() => {
      called = true;
      return ok({} as T);
    };
    expect(await updateCommand(fake, ["--apply"])).toBe(2);
    expect(called).toBe(false);
  });
});
