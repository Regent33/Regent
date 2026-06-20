// `regent memory pending|approve|reject` — the human gate for long-term memory
// writes the agent proposes (security §10.2). Mirrors memory.go.
import { out, printError } from "@app/cli/runtime.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";

export async function memoryCommand(client: IRpcClient, args: string[]): Promise<number> {
  const [sub, ...rest] = args;

  if (sub === "approve") {
    const id = rest[0];
    if (!id) {
      printError("usage: regent memory approve <id>");
      return 1;
    }
    const res = await client.call<{ approved: boolean; node_id: string | null }>(
      "memory.approve",
      { id },
      30_000,
    );
    if (!res.ok) {
      printError(res.error.message);
      return 1;
    }
    if (res.value.approved && res.value.node_id) out(`approved → ${style.teal(res.value.node_id)}`);
    else out(style.grey("no such pending write (already resolved or expired)"));
    return 0;
  }

  if (sub === "reject") {
    const id = rest[0];
    if (!id) {
      printError("usage: regent memory reject <id>");
      return 1;
    }
    const res = await client.call<{ removed: boolean }>("memory.reject", { id }, 30_000);
    if (!res.ok) {
      printError(res.error.message);
      return 1;
    }
    out(res.value.removed ? "rejected" : style.grey("no such pending write"));
    return 0;
  }

  if (sub === "list") {
    const res = await client.call<
      Array<{ id: string; kind: string; name: string; content: string; pinned: boolean }>
    >("memory.list", { limit: 30 }, 30_000);
    if (!res.ok) {
      printError(res.error.message);
      return 1;
    }
    if (res.value.length === 0) {
      out(style.grey("no committed memories yet"));
      return 0;
    }
    for (const n of res.value) {
      const pin = n.pinned ? style.gold("📌") : "  ";
      out(`${pin} ${style.teal(n.id)}  ${style.grey(`[${n.kind}]`)}  ${n.content}`);
    }
    return 0;
  }

  if (sub === "pin" || sub === "unpin") {
    const id = rest[0];
    if (!id) {
      printError(`usage: regent memory ${sub} <id>`);
      return 1;
    }
    const res = await client.call<{ found: boolean }>(`memory.${sub}`, { id }, 30_000);
    if (!res.ok) {
      printError(res.error.message);
      return 1;
    }
    out(res.value.found ? (sub === "pin" ? "pinned" : "unpinned") : style.grey("no such memory"));
    return 0;
  }

  if (sub === "forget") {
    const id = rest[0];
    if (!id) {
      printError("usage: regent memory forget <id>");
      return 1;
    }
    const res = await client.call<{ forgotten: boolean }>("memory.forget", { id }, 30_000);
    if (!res.ok) {
      printError(res.error.message);
      return 1;
    }
    out(res.value.forgotten ? "forgotten" : style.grey("no such memory"));
    return 0;
  }

  // Default: pending.
  const res = await client.call<
    Array<{ id: string; kind: string; provenance: string; trust: number; content: string }>
  >("memory.pending", {}, 30_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  if (res.value.length === 0) {
    out(style.grey("no memory writes awaiting approval"));
    return 0;
  }
  for (const w of res.value) {
    const meta = style.grey(`[${w.kind}/${w.provenance} trust ${w.trust.toFixed(1)}]`);
    out(`${style.teal(w.id)}  ${meta}  ${w.content}`);
  }
  return 0;
}
