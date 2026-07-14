// Interactive field-by-field editor for `agents edit <name>` with no flags —
// the flags-only surface made bare `edit` a silent no-op save. Shows every
// editable field with its current value; Enter keeps it, input replaces it.
// (Skills are not a stored agent field yet — see docs/plans/bug-backlog.)
import { out } from "@app/cli/runtime.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";

export interface AgentFields {
  name: string;
  description: string;
  system_prompt: string;
  model: string;
  tools: string;
}

// Synchronous line prompt via Bun's built-in `prompt` (same pattern as
// `regent setup`). Returns the current value when the user just hits Enter.
function askKeep(label: string, current: string, hint: string): string {
  out(`  ${style.grey(hint)}`);
  const shown = current === "" ? style.grey("(empty)") : style.value(truncate(current, 60));
  out(`  ${style.grey("current:")} ${shown}`);
  const answer = (prompt(`  ${label} [Enter keeps]:`) ?? "").trim();
  out("");
  return answer === "" ? current : answer;
}

const truncate = (s: string, n: number): string => (s.length > n ? `${s.slice(0, n - 1)}…` : s);

export async function editInteractive(
  client: IRpcClient,
  current: AgentFields,
): Promise<number> {
  out("");
  out(style.heading(`Edit agent — ${current.name}`));
  out(style.grey("Enter keeps the current value. Long prompts: use --prompt \"...\" instead."));
  out("");

  const next: AgentFields = {
    name: current.name,
    description: askKeep("Description", current.description, "one line: what this agent is for"),
    system_prompt: askKeep("System prompt", current.system_prompt, "the agent's standing instructions"),
    model: askKeep("Model", current.model, "provider/model — empty inherits the session model"),
    tools: askKeep("Tools", current.tools, "comma-separated tool names — empty allows the full catalog"),
  };

  out(style.heading("Review"));
  out(`  ${style.grey("description")} ${next.description || style.grey("(empty)")}`);
  out(`  ${style.grey("model      ")} ${next.model || style.grey("(inherits session model)")}`);
  out(`  ${style.grey("tools      ")} ${next.tools || style.grey("(full catalog)")}`);
  out(`  ${style.grey("prompt     ")} ${truncate(next.system_prompt, 70) || style.grey("(none)")}`);
  const confirm = (prompt("  Save? [Y/n]:") ?? "").trim().toLowerCase();
  if (confirm === "n" || confirm === "no") {
    out(style.grey("not saved"));
    return 0;
  }

  const res = await client.call<{ ok: boolean }>("agents.set", { ...next }, 15_000);
  if (!res.ok) {
    out(style.fail(`✗ ${res.error.message}`));
    return 1;
  }
  out(`${style.pass("✓")} agent ${style.value(next.name)} saved`);
  return 0;
}
