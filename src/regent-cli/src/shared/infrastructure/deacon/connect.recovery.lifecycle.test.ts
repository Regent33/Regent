import { expect, test } from "bun:test";
import type { IRpcClient, RpcFailure } from "@shared/kernel/contracts.ts";
import { err, ok, type Result } from "@shared/kernel/result.ts";
import type { HealthyDeacon } from "./connect.ts";
import { withHealthRecovery } from "./connect.ts";

const closedFailure = err({ kind: "rpc", message: "health: client is closed" });

function clientThatFailsWhenClosed(): {
  client: IRpcClient;
  calls: () => number;
  closes: () => number;
} {
  let closed = false;
  let calls = 0;
  let closes = 0;
  return {
    client: {
      call: <T = unknown>() => {
        calls += 1;
        return Promise.resolve(
          (closed ? closedFailure : ok({ healthy: true })) as Result<T, RpcFailure>,
        );
      },
      onNotification: () => () => {},
      close: () => {
        closed = true;
        closes += 1;
        return Promise.resolve();
      },
    },
    calls: () => calls,
    closes: () => closes,
  };
}

test("close prevents a later health call from reconnecting the wrapper", async () => {
  const initial = clientThatFailsWhenClosed();
  let reconnects = 0;
  const reconnect = (): Promise<Result<HealthyDeacon>> => {
    reconnects += 1;
    return Promise.resolve(ok({ client: clientThatFailsWhenClosed().client, path: "/fresh" }));
  };
  const client = withHealthRecovery({ client: initial.client, path: "/stale" }, reconnect);

  await client.close();
  const res = await client.call("health");

  expect(res.ok).toBe(false);
  expect(reconnects).toBe(0);
  expect(initial.closes()).toBe(1);
});

test("close during reconnect closes the replacement without adopting or replaying it", async () => {
  const dead: IRpcClient = {
    call: <T = unknown>() => Promise.resolve(closedFailure as Result<T, RpcFailure>),
    onNotification: () => () => {},
    close: () => Promise.resolve(),
  };
  const replacement = clientThatFailsWhenClosed();
  let resolveReconnect: ((value: Result<HealthyDeacon>) => void) | undefined;
  let markStarted: (() => void) | undefined;
  const started = new Promise<void>((resolve) => {
    markStarted = resolve;
  });
  const reconnect = (): Promise<Result<HealthyDeacon>> => {
    markStarted?.();
    return new Promise((resolve) => {
      resolveReconnect = resolve;
    });
  };
  const client = withHealthRecovery({ client: dead, path: "/stale" }, reconnect);

  const pending = client.call("health");
  await started;
  await client.close();
  resolveReconnect?.(ok({ client: replacement.client, path: "/fresh" }));
  const res = await pending;

  expect(res.ok).toBe(false);
  expect(replacement.calls()).toBe(0);
  expect(replacement.closes()).toBe(1);
});
