// `regent kanban …` — the shared work board (kanban.* on the "default" board):
// list / create / show / assign / block / unblock / complete.
import { out, printError } from "@app/cli/runtime.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";

interface Task {
  id: string;
  board: string;
  title: string;
  description: string;
  status: string;
  assignee: string | null;
  created_at: number;
  updated_at: number;
}

const STATUS_PAINT: Record<string, (s: string) => string> = {
  todo: style.grey,
  in_progress: style.teal,
  in_review: style.warn,
  blocked: style.fail,
  done: style.pass,
};

const paintStatus = (s: string): string => (STATUS_PAINT[s] ?? ((x: string) => x))(s);
const shortId = (id: string): string => (id.length > 13 ? `${id.slice(0, 13)}…` : id);

export async function kanbanCommand(client: IRpcClient, args: string[]): Promise<number> {
  const [sub = "list", ...rest] = args;
  switch (sub) {
    case "list":
      return list(client, rest[0]);
    case "create":
    case "add":
      return create(client, rest.join(" "));
    case "show":
      return show(client, rest[0]);
    case "assign":
      return assign(client, rest[0], rest[1]);
    case "block":
      return setStatus(client, rest[0], "blocked", "blocked", "block");
    case "unblock":
      return setStatus(client, rest[0], "todo", "unblocked", "unblock");
    case "complete":
    case "done":
      return setStatus(client, rest[0], "done", "completed", "complete");
    default:
      printError(`unknown kanban subcommand: ${sub}`);
      out(
        "usage: kanban [list [status] | create <title> | show <id> | assign <id> <worker> | block|unblock|complete <id>]",
      );
      return 1;
  }
}

async function list(client: IRpcClient, status: string | undefined): Promise<number> {
  const res = await client.call<Task[]>("kanban.list", status ? { status } : {}, 15_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  if (res.value.length === 0) {
    out(style.grey(status ? `no "${status}" tasks` : "board is empty — kanban create <title>"));
    return 0;
  }
  out(style.heading(`Board "default" — ${res.value.length} task(s)`));
  for (const t of res.value) {
    const who = t.assignee ? style.grey(` @${t.assignee}`) : "";
    out(`  ${style.grey(shortId(t.id))}  ${paintStatus(t.status.padEnd(11))} ${t.title}${who}`);
  }
  return 0;
}

async function create(client: IRpcClient, title: string): Promise<number> {
  if (!title.trim()) {
    printError("usage: kanban create <title>");
    return 1;
  }
  const res = await client.call<{ id: string }>("kanban.create", { title }, 15_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  out(`${style.pass("✓ created")} ${style.value(res.value.id)}`);
  return 0;
}

async function show(client: IRpcClient, id: string | undefined): Promise<number> {
  if (!id) {
    printError("usage: kanban show <id>");
    return 1;
  }
  const res = await client.call<Task>("kanban.show", { id }, 15_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  const t = res.value;
  out(style.heading(t.title));
  out(`  ${style.grey("id      ")} ${t.id}`);
  out(`  ${style.grey("status  ")} ${paintStatus(t.status)}`);
  out(`  ${style.grey("assignee")} ${t.assignee ?? "—"}`);
  if (t.description) out(`  ${style.grey("details ")} ${t.description}`);
  return 0;
}

async function assign(
  client: IRpcClient,
  id: string | undefined,
  worker: string | undefined,
): Promise<number> {
  if (!id || !worker) {
    printError("usage: kanban assign <id> <worker>");
    return 1;
  }
  const res = await client.call<{ claimed: boolean }>("kanban.assign", { id, worker }, 15_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  if (!res.value.claimed) {
    printError(`${id} could not be claimed (not in 'todo', or gone)`);
    return 1;
  }
  out(`${style.pass("✓")} ${id} assigned to ${style.value(worker)}`);
  return 0;
}

async function setStatus(
  client: IRpcClient,
  id: string | undefined,
  status: string,
  verb: string,
  usage: string,
): Promise<number> {
  if (!id) {
    printError(`usage: kanban ${usage} <id>`);
    return 1;
  }
  const res = await client.call<{ ok: boolean }>("kanban.set_status", { id, status }, 15_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  if (!res.value.ok) {
    printError(`no task ${id}`);
    return 1;
  }
  out(`${style.pass("✓")} ${id} ${verb}`);
  return 0;
}
