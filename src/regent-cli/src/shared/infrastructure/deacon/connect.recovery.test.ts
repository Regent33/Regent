// Regression coverage for the EPIPE repro: `regent` spawns one child per
// invocation, and if it dies between the initial health probe and the first
// real call the bare transport failure ("write health: EPIPE: broken pipe,
// write") used to reach the user as "Couldn't reach the deacon" with no
// recovery. `withHealthRecovery` may replay only that idempotent health check;
// non-health RPCs are never replayed because they may have already performed a
// side effect before their transport closed.
import { expect, test } from "bun:test";
import type { IRpcClient, RpcFailure, RpcNotification } from "@shared/kernel/contracts.ts";
import { err, ok, type Result } from "@shared/kernel/result.ts";
import type { HealthyDeacon } from "./connect.ts";
import { withHealthRecovery } from "./connect.ts";

// A fake client that answers `call()` from a canned queue (last entry repeats)
// and tracks how many times it was closed.
function fakeClient(responses: ReadonlyArray<Result<unknown, RpcFailure>>): {
  client: IRpcClient;
  calls: () => number;
  closed: () => number;
} {
  let i = 0;
  let closed = 0;
  const client: IRpcClient = {
    call: <T = unknown>() =>
      Promise.resolve(responses[Math.min(i++, responses.length - 1)] as Result<T, RpcFailure>),
    onNotification: (_h: (n: RpcNotification) => void) => () => {},
    close: () => {
      closed += 1;
      return Promise.resolve();
    },
  };
  return { client, calls: () => i, closed: () => closed };
}

const connected = (client: IRpcClient, path = "/stale"): HealthyDeacon => ({ client, path });

const transportFailure: Result<unknown, RpcFailure> = err({
  kind: "rpc",
  message: "write health: EPIPE: broken pipe, write",
});

test("recovers once from a dead-transport failure by reconnecting and replaying health", async () => {
  const dead = fakeClient([transportFailure]);
  const alive = fakeClient([ok({ echo: "health" })]);
  let reconnects = 0;
  const reconnect = (failedPath: string): Promise<Result<HealthyDeacon>> => {
    expect(failedPath).toBe("/stale");
    reconnects += 1;
    return Promise.resolve(ok({ client: alive.client, path: "/fresh" }));
  };

  const client = withHealthRecovery(connected(dead.client), reconnect);
  const res = await client.call("health", {}, 10_000);

  expect(res.ok).toBe(true);
  if (res.ok) expect(res.value).toEqual({ echo: "health" });
  expect(reconnects).toBe(1);
  expect(dead.closed()).toBe(1); // the dead client was torn down
  expect(alive.calls()).toBe(1); // the call was replayed exactly once
});

test("a timeout is never replayed", async () => {
  const timedOut = fakeClient([err({ kind: "rpc", message: "health: timed out after 50ms" })]);
  let reconnects = 0;
  const reconnect = (): Promise<Result<HealthyDeacon>> => {
    reconnects += 1;
    throw new Error("must not be called for a timeout");
  };

  const client = withHealthRecovery(connected(timedOut.client), reconnect);
  const res = await client.call("health", {}, 50);

  expect(res.ok).toBe(false);
  if (!res.ok) expect(res.error.message).toContain("timed out");
  expect(reconnects).toBe(0);
  expect(timedOut.closed()).toBe(0); // never torn down — no recovery attempted
});

test("a JSON-RPC application error is never replayed", async () => {
  const appError = fakeClient([err({ kind: "rpc", message: "nope", code: -32601 })]);
  let reconnects = 0;
  const reconnect = (): Promise<Result<HealthyDeacon>> => {
    reconnects += 1;
    throw new Error("must not be called for an application error");
  };

  const client = withHealthRecovery(connected(appError.client), reconnect);
  const res = await client.call("health", {}, 10_000);

  expect(res.ok).toBe(false);
  if (!res.ok) {
    expect(res.error.code).toBe(-32601);
    expect(res.error.message).toBe("nope");
  }
  expect(reconnects).toBe(0);
});

test("at most one reconnect per health call returns a friendly failure if the replay also dies", async () => {
  const dead = fakeClient([transportFailure]);
  const stillDead = fakeClient([transportFailure]);
  let reconnects = 0;
  const reconnect = (): Promise<Result<HealthyDeacon>> => {
    reconnects += 1;
    return Promise.resolve(ok({ client: stillDead.client, path: "/still-bad" }));
  };

  const client = withHealthRecovery(connected(dead.client), reconnect);
  const res = await client.call("health", {}, 10_000);

  expect(res.ok).toBe(false);
  if (!res.ok) {
    expect(res.error.message).toBe(
      "regent-deacon stopped during startup; run `regent doctor` for details",
    );
  }
  expect(reconnects).toBe(1);
  expect(stillDead.calls()).toBe(1);
});

test("never reconnects or replays a non-health transport failure", async () => {
  const dead = fakeClient([transportFailure]);
  const alive = fakeClient([ok({ duplicated: true })]);
  let reconnects = 0;
  const reconnect = (): Promise<Result<HealthyDeacon>> => {
    reconnects += 1;
    return Promise.resolve(ok({ client: alive.client, path: "/fresh" }));
  };

  const client = withHealthRecovery(connected(dead.client), reconnect);
  const res = await client.call("prompt.submit", { text: "hello" }, 0);

  expect(res.ok).toBe(false);
  expect(reconnects).toBe(0);
  expect(alive.calls()).toBe(0);
});

test("when reconnecting also fails, the friendly candidate report is surfaced", async () => {
  const dead = fakeClient([transportFailure]);
  const reconnectFailure =
    "no working regent-deacon among 2 candidate(s):\n  /stale: fatal: yaml: model: unknown field `review`\n  /fresh: ECONNRESET";
  const reconnect = (): Promise<Result<HealthyDeacon>> =>
    Promise.resolve(err({ kind: "deacon-spawn", message: reconnectFailure }));

  const client = withHealthRecovery(connected(dead.client), reconnect);
  const res = await client.call("health", {}, 10_000);

  expect(res.ok).toBe(false);
  if (!res.ok) expect(res.error.message).toBe(reconnectFailure);
});

test("onNotification re-subscribes handlers to the reconnected client", async () => {
  const dead = fakeClient([transportFailure]);
  const alive = fakeClient([ok(undefined)]);
  const notifyHandlers: Array<(n: RpcNotification) => void> = [];
  alive.client.onNotification = (h) => {
    notifyHandlers.push(h);
    return () => {};
  };
  const reconnect = (): Promise<Result<HealthyDeacon>> =>
    Promise.resolve(ok({ client: alive.client, path: "/fresh" }));

  const client = withHealthRecovery(connected(dead.client), reconnect);
  const seen: string[] = [];
  client.onNotification((n) => seen.push(n.method));
  await client.call("health", {}, 10_000);

  expect(notifyHandlers.length).toBe(1);
  notifyHandlers[0]?.({ method: "turn.started", params: {} });
  expect(seen).toEqual(["turn.started"]);
});
