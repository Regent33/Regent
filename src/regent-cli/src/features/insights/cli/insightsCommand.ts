// `regent insights` — a usage rollup across every session: turns, token
// spend, and api calls drawn from the turns ledger (insights.get).
import { out, printError } from "@app/cli/runtime.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";

interface Insights {
  sessions: number;
  turns: number;
  turns_ok: number;
  input_tokens: number;
  output_tokens: number;
  api_calls: number;
  messages: number;
}

const n = (v: number): string => v.toLocaleString("en-US");

export async function insightsCommand(client: IRpcClient): Promise<number> {
  const res = await client.call<Insights>("insights.get", {}, 15_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  const i = res.value;
  const failed = i.turns - i.turns_ok;
  out(style.heading("Regent insights"));
  out(`  ${"sessions".padEnd(13)} ${n(i.sessions)}`);
  out(`  ${"messages".padEnd(13)} ${n(i.messages)}`);
  out(
    `  ${"turns".padEnd(13)} ${n(i.turns)}  ${style.grey(`(${n(i.turns_ok)} ok · ${n(failed)} failed)`)}`,
  );
  out(`  ${"api calls".padEnd(13)} ${n(i.api_calls)}`);
  out(`  ${"tokens in".padEnd(13)} ${n(i.input_tokens)}`);
  out(`  ${"tokens out".padEnd(13)} ${n(i.output_tokens)}`);
  out(`  ${"tokens total".padEnd(13)} ${style.value(n(i.input_tokens + i.output_tokens))}`);
  return 0;
}
