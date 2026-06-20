// Newline-delimited JSON-RPC 2.0 over a reader/writer pair — the daemon's
// stdio transport (ADR-011). Constructor injection: tests pass in-memory
// streams, production passes the spawned daemon's stdio. Ported in spirit
// from the Go rpc.Client; same wire protocol, idiomatic TS.
import { type Interface, createInterface } from "node:readline";
import type { Readable, Writable } from "node:stream";
import type { IRpcClient, RpcFailure, RpcNotification } from "@shared/kernel/contracts.ts";
import { type Result, err, ok } from "@shared/kernel/result.ts";

const DEFAULT_TIMEOUT_MS = 30_000;

interface Pending {
  resolve: (value: Result<unknown, RpcFailure>) => void;
  // undefined when the call opted out of a timeout (e.g. prompt.submit, which
  // resolves only when the turn ends — possibly minutes later).
  timer: ReturnType<typeof setTimeout> | undefined;
}

interface InboundMessage {
  jsonrpc?: string;
  result?: unknown;
  error?: { code: number; message: string };
  id?: number | null;
  method?: string;
  params?: Record<string, unknown>;
}

const rpcFailure = (message: string, code?: number): RpcFailure =>
  code === undefined ? { kind: "rpc", message } : { kind: "rpc", message, code };

export class RpcClient implements IRpcClient {
  private nextId = 0;
  private readonly pending = new Map<number, Pending>();
  private readonly handlers = new Set<(n: RpcNotification) => void>();
  private readonly reader: Interface;
  private closed = false;

  constructor(
    input: Readable,
    private readonly output: Writable,
    private readonly onClose?: () => Promise<void> | void,
  ) {
    this.reader = createInterface({ input, crlfDelay: Number.POSITIVE_INFINITY });
    this.reader.on("line", (line) => this.demux(line));
    this.reader.on("close", () => this.drainPending());
  }

  call<T = unknown>(
    method: string,
    params: Record<string, unknown> = {},
    timeoutMs = DEFAULT_TIMEOUT_MS,
  ): Promise<Result<T, RpcFailure>> {
    if (this.closed) return Promise.resolve(err(rpcFailure(`${method}: client is closed`)));
    const id = ++this.nextId;
    return new Promise((resolve) => {
      // timeoutMs <= 0 opts out of the timeout (long-running calls like
      // prompt.submit); such calls settle on response or on stream close.
      const timer =
        timeoutMs > 0
          ? setTimeout(() => {
              this.pending.delete(id);
              resolve(err(rpcFailure(`${method}: timed out after ${timeoutMs}ms`)));
            }, timeoutMs)
          : undefined;
      this.pending.set(id, { resolve: resolve as Pending["resolve"], timer });

      const line = `${JSON.stringify({ jsonrpc: "2.0", method, params, id })}\n`;
      this.output.write(line, (writeErr) => {
        if (!writeErr) return;
        this.settle(id, err(rpcFailure(`write ${method}: ${writeErr.message}`)));
      });
    });
  }

  onNotification(handler: (n: RpcNotification) => void): () => void {
    this.handlers.add(handler);
    return () => this.handlers.delete(handler);
  }

  async close(): Promise<void> {
    if (this.closed) return;
    this.closed = true;
    this.reader.close();
    await this.onClose?.();
  }

  private demux(line: string): void {
    let msg: InboundMessage;
    try {
      msg = JSON.parse(line) as InboundMessage;
    } catch {
      return; // non-protocol noise on stdout is dropped, like the Go demux
    }
    if (msg.method) {
      const n: RpcNotification = { method: msg.method, params: msg.params ?? {} };
      for (const handler of this.handlers) handler(n);
      return;
    }
    if (typeof msg.id !== "number") return;
    if (msg.error) {
      this.settle(msg.id, err(rpcFailure(msg.error.message, msg.error.code)));
      return;
    }
    this.settle(msg.id, ok(msg.result));
  }

  private settle(id: number, result: Result<unknown, RpcFailure>): void {
    const p = this.pending.get(id);
    if (!p) return;
    clearTimeout(p.timer);
    this.pending.delete(id);
    p.resolve(result);
  }

  private drainPending(): void {
    for (const [id] of this.pending) {
      this.settle(id, err(rpcFailure("daemon stream closed")));
    }
  }
}
