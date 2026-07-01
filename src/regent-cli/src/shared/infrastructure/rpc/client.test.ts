import { expect, test } from "bun:test";
import { createInterface } from "node:readline";
import { PassThrough } from "node:stream";
import type { RpcNotification } from "@shared/kernel/contracts.ts";
import { RpcClient } from "./client.ts";

// Wire an in-memory transport: the client reads `deaconToClient` and writes
// `clientToDeacon` (mirrors the Go pipe-pair test).
function wire() {
  const clientToDeacon = new PassThrough();
  const deaconToClient = new PassThrough();
  const client = new RpcClient(deaconToClient, clientToDeacon);
  return { client, clientToDeacon, deaconToClient };
}

// Answers every request with a canned echo result; can inject notifications
// before the response — the order the chat surface relies on.
function fakeDeacon(req: PassThrough, resp: PassThrough, notifyFirst: string[] = []) {
  const rl = createInterface({ input: req });
  rl.on("line", (line) => {
    const r = JSON.parse(line) as { method: string; id: number };
    for (const method of notifyFirst) {
      resp.write(`${JSON.stringify({ jsonrpc: "2.0", method, params: { session_id: "s1" } })}\n`);
    }
    resp.write(`${JSON.stringify({ jsonrpc: "2.0", result: { echo: r.method }, id: r.id })}\n`);
  });
}

test("call routes the response back to its caller by id", async () => {
  const { client, clientToDeacon, deaconToClient } = wire();
  fakeDeacon(clientToDeacon, deaconToClient);

  const res = await client.call<{ echo: string }>("health", {}, 5_000);
  expect(res.ok).toBe(true);
  if (res.ok) expect(res.value.echo).toBe("health");
});

test("notifications fan out to registered handlers", async () => {
  const { client, clientToDeacon, deaconToClient } = wire();
  fakeDeacon(clientToDeacon, deaconToClient, ["turn.started", "tool.start"]);

  const got: string[] = [];
  client.onNotification((n: RpcNotification) => got.push(n.method));

  const res = await client.call("prompt.submit", { text: "hi" }, 5_000);
  expect(res.ok).toBe(true);
  expect(got).toEqual(["turn.started", "tool.start"]);
});

test("error responses surface as a typed rpc failure", async () => {
  const { client, clientToDeacon, deaconToClient } = wire();
  const rl = createInterface({ input: clientToDeacon });
  rl.on("line", (line) => {
    const r = JSON.parse(line) as { id: number };
    deaconToClient.write(
      `${JSON.stringify({ jsonrpc: "2.0", error: { code: -32601, message: "nope" }, id: r.id })}\n`,
    );
  });

  const res = await client.call("no.such", {}, 5_000);
  expect(res.ok).toBe(false);
  if (!res.ok) {
    expect(res.error.kind).toBe("rpc");
    expect(res.error.code).toBe(-32601);
  }
});

test("a call times out with a typed failure when no response arrives", async () => {
  const { client } = wire(); // no fake deacon → nothing ever answers
  const res = await client.call("health", {}, 50);
  expect(res.ok).toBe(false);
  if (!res.ok) expect(res.error.message).toContain("timed out");
});
