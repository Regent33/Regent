// `regent sessions list [--limit N]` and `regent sessions search <query>`.
// Mirrors sessions.go.
import { parseFlags } from "@app/cli/args.ts";
import { out, printError } from "@app/cli/runtime.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";

const fmtTime = (epoch: number): string =>
  new Date(epoch * 1000).toISOString().slice(0, 16).replace("T", " ");

export async function sessionsCommand(client: IRpcClient, args: string[]): Promise<number> {
  if (args[0] === "search") {
    const query = args[1];
    if (!query) {
      printError("usage: regent sessions search <query>");
      return 1;
    }
    const res = await client.call<Array<{ session_id: string; role: string; snippet: string }>>(
      "session.search",
      { query, limit: 20 },
      30_000,
    );
    if (!res.ok) {
      printError(res.error.message);
      return 1;
    }
    if (res.value.length === 0) {
      out(style.grey("no matches"));
      return 0;
    }
    for (const h of res.value) {
      out(`${style.teal(h.session_id)} ${style.grey(`[${h.role}]`)} ${h.snippet}`);
    }
    return 0;
  }

  // Default: list. Accept `sessions list --limit N` or `sessions --limit N`.
  const listArgs = args[0] === "list" ? args.slice(1) : args;
  const { values } = parseFlags(listArgs, { limit: { type: "string" } });
  const limit = Number(values.limit) || 20;

  const res = await client.call<
    Array<{
      session_id: string;
      source: string;
      model: string | null;
      message_count: number;
      started_at: number;
    }>
  >("session.list", { limit }, 30_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  if (res.value.length === 0) {
    out(style.grey("no sessions yet"));
    return 0;
  }
  for (const s of res.value) {
    const msgs = String(s.message_count).padStart(3);
    out(
      `${style.teal(s.session_id)}  ${(s.source ?? "-").padEnd(8)}  ${(s.model ?? "-").padEnd(24)}  ${msgs} msgs  ${style.grey(fmtTime(s.started_at))}`,
    );
  }
  return 0;
}
