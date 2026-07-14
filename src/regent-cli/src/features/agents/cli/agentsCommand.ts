// `regent agents list|create|show|edit|remove` — persistent, reusable named
// agent definitions (name · role · system prompt · optional model/tools). A
// kanban task assigned to <name> is worked by that agent (the board dispatcher
// resolves assignee → definition). The deacon owns the store; the CLI talks
// agents.* over RPC.
import { parseFlags } from "@app/cli/args.ts";
import { out, printError } from "@app/cli/runtime.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";
import { renderTable } from "@shared/ui/table.ts";

interface Agent {
  name: string;
  description: string;
  system_prompt: string;
  model: string | null;
  tools: string | null;
  created_at: number;
  updated_at: number;
}

const FLAGS = {
  description: { type: "string", alias: "d" },
  prompt: { type: "string" },
  model: { type: "string", alias: "m" },
  tools: { type: "string", alias: "t" },
} as const;

export async function agentsCommand(client: IRpcClient, args: string[]): Promise<number> {
  const [sub = "list", ...rest] = args;
  switch (sub) {
    case "list":
      return list(client);
    case "create":
    case "add":
      return create(client, rest);
    case "edit":
      return edit(client, rest);
    case "show":
      return show(client, rest[0]);
    case "remove":
    case "rm":
    case "delete":
      return remove(client, rest[0]);
    default:
      printError(`unknown agents subcommand: ${sub}`);
      out(
        'usage: agents [list | create <name> --description "..." --prompt "..." [--model m] [--tools a,b] | show <name> | edit <name> [flags] | remove <name>]',
      );
      return 1;
  }
}

async function list(client: IRpcClient): Promise<number> {
  const res = await client.call<Agent[]>("agents.list", {}, 15_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  if (res.value.length === 0) {
    out(style.grey('no agents — agents create <name> --description "..." --prompt "..."'));
    return 0;
  }
  out(style.heading(`Agents — ${res.value.length}`));
  for (const line of renderTable(res.value, [
    { header: "NAME", get: (a) => a.name, paint: (c) => style.teal(c) },
    { header: "MODEL", get: (a) => a.model ?? "—", paint: (c) => style.grey(c) },
    { header: "TOOLS", get: (a) => a.tools ?? "(all)", paint: (c) => style.grey(c) },
    { header: "ROLE", get: (a) => a.description, flex: true },
  ])) {
    out(line);
  }
  return 0;
}

async function create(client: IRpcClient, rest: string[]): Promise<number> {
  const { values, positionals } = parseFlags(rest, FLAGS);
  const name = positionals[0];
  if (!name) {
    printError('usage: agents create <name> --description "..." --prompt "..."');
    return 1;
  }
  const exists = await client.call<Agent>("agents.show", { name }, 15_000);
  if (exists.ok) {
    printError(`agent '${name}' already exists — use \`agents edit ${name}\``);
    return 1;
  }
  return setAgent(client, {
    name,
    description: str(values.description),
    system_prompt: str(values.prompt),
    model: str(values.model),
    tools: str(values.tools),
  });
}

async function edit(client: IRpcClient, rest: string[]): Promise<number> {
  const { values, positionals } = parseFlags(rest, FLAGS);
  const name = positionals[0];
  if (!name) {
    printError(
      "usage: agents edit <name> [--description ...] [--prompt ...] [--model ...] [--tools ...]",
    );
    return 1;
  }
  const cur = await client.call<Agent>("agents.show", { name }, 15_000);
  if (!cur.ok) {
    printError(cur.error.message);
    return 1;
  }
  const c = cur.value;
  // Bare `edit <name>` in a terminal → the field-by-field editor (flags-only
  // edit used to silently re-save unchanged values). Flags stay scriptable.
  const anyFlag =
    values.description !== undefined ||
    values.prompt !== undefined ||
    values.model !== undefined ||
    values.tools !== undefined;
  if (!anyFlag) {
    if (!process.stdin.isTTY) {
      printError(
        "nothing to change — pass at least one of --description/--prompt/--model/--tools (interactive editor needs a terminal)",
      );
      return 1;
    }
    const { editInteractive } = await import("./agentsEditInteractive.ts");
    return editInteractive(client, {
      name,
      description: c.description,
      system_prompt: c.system_prompt,
      model: c.model ?? "",
      tools: c.tools ?? "",
    });
  }
  // Merge: a flag overrides; everything else keeps its current value.
  return setAgent(client, {
    name,
    description: values.description !== undefined ? str(values.description) : c.description,
    system_prompt: values.prompt !== undefined ? str(values.prompt) : c.system_prompt,
    model: values.model !== undefined ? str(values.model) : (c.model ?? ""),
    tools: values.tools !== undefined ? str(values.tools) : (c.tools ?? ""),
  });
}

async function setAgent(
  client: IRpcClient,
  params: {
    name: string;
    description: string;
    system_prompt: string;
    model: string;
    tools: string;
  },
): Promise<number> {
  const res = await client.call<{ ok: boolean }>("agents.set", params, 15_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  out(`${style.pass("✓")} agent ${style.value(params.name)} saved`);
  return 0;
}

async function show(client: IRpcClient, name: string | undefined): Promise<number> {
  if (!name) {
    printError("usage: agents show <name>");
    return 1;
  }
  const res = await client.call<Agent>("agents.show", { name }, 15_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  const a = res.value;
  out(style.heading(a.name));
  out(`  ${style.grey("role   ")} ${a.description || "—"}`);
  out(`  ${style.grey("model  ")} ${a.model ?? style.grey("(inherits session model)")}`);
  out(`  ${style.grey("tools  ")} ${a.tools ?? style.grey("(full catalog)")}`);
  // Multi-line prompts render as an indented block, not one squashed line.
  if (a.system_prompt) {
    out(`  ${style.grey("prompt ")}`);
    for (const line of a.system_prompt.split("\n")) out(`    ${line}`);
  } else {
    out(`  ${style.grey("prompt ")} ${style.grey("(none)")}`);
  }
  return 0;
}

async function remove(client: IRpcClient, name: string | undefined): Promise<number> {
  if (!name) {
    printError("usage: agents remove <name>");
    return 1;
  }
  const res = await client.call<{ removed: boolean }>("agents.remove", { name }, 15_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  out(res.value.removed ? `${style.pass("✓")} removed ${name}` : style.grey(`no agent '${name}'`));
  return 0;
}

const str = (v: string | boolean | undefined): string => (typeof v === "string" ? v : "");
