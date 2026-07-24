import { describe, expect, test } from "bun:test";
import type { IRpcClient, RpcFailure } from "@shared/kernel/contracts.ts";
import { type Result, err, ok } from "@shared/kernel/result.ts";
import { checkForUpdateNotice, fetchUpdateStatus } from "./checkForUpdate.ts";

// A fake deacon whose single `update.status` answer is scripted per test. `call`
// is generic over the method so unrelated calls (none here) stay well-typed.
function fakeClient(answer: Result<unknown, RpcFailure> | (() => never)): IRpcClient {
  return {
    call: <T = unknown>(): Promise<Result<T, RpcFailure>> => {
      if (typeof answer === "function") answer(); // throw-on-call fake
      return Promise.resolve(answer as Result<T, RpcFailure>);
    },
    onNotification: () => () => {},
    close: () => Promise.resolve(),
  };
}

describe("fetchUpdateStatus", () => {
  test("parses a well-formed cached verdict", async () => {
    const client = fakeClient(ok({ current: "0.1.1", latest: "0.2.0", available: true }));
    const s = await fetchUpdateStatus(client, 50);
    expect(s?.available).toBe(true);
    expect(s?.latest).toBe("0.2.0");
  });

  test("an old deacon without the method (rpc error) yields null, not a throw", async () => {
    const client = fakeClient(err({ kind: "rpc", message: "method not found: update.status" }));
    expect(await fetchUpdateStatus(client, 50)).toBeNull();
  });

  test("a malformed body yields null", async () => {
    const client = fakeClient(ok({ nonsense: true }));
    expect(await fetchUpdateStatus(client, 50)).toBeNull();
  });

  test("a throwing transport is swallowed to null", async () => {
    const client = fakeClient(() => {
      throw new Error("stream closed");
    });
    expect(await fetchUpdateStatus(client, 50)).toBeNull();
  });
});

describe("checkForUpdateNotice", () => {
  test("returns an actionable line when an upgrade is available", async () => {
    const client = fakeClient(ok({ current: "0.1.1", latest: "0.2.0", available: true }));
    const line = await checkForUpdateNotice(client, "0.1.1", 50);
    expect(line).toContain("0.2.0");
    expect(line).toContain("0.1.1");
  });

  test("returns null when nothing is available", async () => {
    const client = fakeClient(ok({ current: "0.1.1", latest: null, available: false }));
    expect(await checkForUpdateNotice(client, "0.1.1", 50)).toBeNull();
  });

  test("returns null (never fails) when the deacon errors", async () => {
    const client = fakeClient(err({ kind: "rpc", message: "boom" }));
    expect(await checkForUpdateNotice(client, "0.1.1", 50)).toBeNull();
  });
});
