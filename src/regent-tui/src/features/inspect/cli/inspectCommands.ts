// `regent model [list|set <id>]`, `regent skills [view|create|opt-out]`,
// `regent config` — inspection + light authoring. Mirrors inspect.go (extended).
import { readFileSync } from "node:fs";
import { parseFlags } from "@app/cli/args.ts";
import { out, printError } from "@app/cli/runtime.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";

export async function modelCommand(client: IRpcClient, args: string[]): Promise<number> {
  const [sub, ...rest] = args;

  if (sub === "list") {
    const res = await client.call<Array<{ id: string; display_name: string; current: boolean }>>(
      "model.list",
      {},
      30_000,
    );
    if (!res.ok) {
      printError(res.error.message);
      return 1;
    }
    for (const m of res.value) {
      const marker = m.current ? style.teal("*") : " ";
      out(`${marker} ${m.id.padEnd(20)} ${style.grey(m.display_name)}`);
    }
    return 0;
  }

  if (sub === "set") {
    const id = rest[0];
    if (!id) {
      printError("usage: regent model set <model-id>");
      return 1;
    }
    const res = await client.call<{ model: string; note: string }>(
      "model.set",
      { model: id },
      30_000,
    );
    if (!res.ok) {
      printError(res.error.message);
      return 1;
    }
    out(`model set to ${style.value(res.value.model)}`);
    out(style.grey(`(${res.value.note})`));
    return 0;
  }

  const res = await client.call<{ model: string }>("model.get", {}, 30_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  out(res.value.model);
  return 0;
}

export async function skillsCommand(client: IRpcClient, args: string[]): Promise<number> {
  const [sub, ...rest] = args;

  if (sub === "view") {
    const name = rest[0];
    if (!name) {
      printError("usage: regent skills view <name>");
      return 1;
    }
    const res = await client.call<{
      name: string;
      description: string;
      tags: string[];
      body: string;
    }>("skills.view", { name }, 30_000);
    if (!res.ok) {
      printError(res.error.message);
      return 1;
    }
    out(`${style.heading(res.value.name)}  ${style.grey(res.value.tags.join(" "))}`);
    out(style.grey(res.value.description));
    out("");
    out(res.value.body);
    return 0;
  }

  if (sub === "create") {
    const { values, positionals } = parseFlags(rest, {
      description: { type: "string" },
      body: { type: "string" },
      file: { type: "string" },
    });
    const name = positionals[0];
    const description = typeof values.description === "string" ? values.description : "";
    let body = typeof values.body === "string" ? values.body : "";
    if (!body && typeof values.file === "string") {
      try {
        body = readFileSync(values.file, "utf8");
      } catch (e) {
        printError(`cannot read ${values.file}: ${e instanceof Error ? e.message : String(e)}`);
        return 1;
      }
    }
    if (!name || !description || !body) {
      printError(
        "usage: regent skills create <name> --description <d> (--body <text> | --file <path>)",
      );
      return 1;
    }
    const res = await client.call<{ created: string }>(
      "skills.create",
      { name, description, body },
      30_000,
    );
    if (!res.ok) {
      printError(res.error.message);
      return 1;
    }
    out(`created skill ${style.teal(res.value.created)}`);
    return 0;
  }

  if (sub === "opt-out") {
    const name = rest[0];
    if (!name) {
      printError("usage: regent skills opt-out <name>");
      return 1;
    }
    const res = await client.call<{ archived: string }>("skills.opt_out", { name }, 30_000);
    if (!res.ok) {
      printError(res.error.message);
      return 1;
    }
    out(`opted out (archived) ${style.teal(res.value.archived)}`);
    return 0;
  }

  // Default: list.
  const res = await client.call<Array<{ name: string; description: string }>>(
    "skills.list",
    {},
    30_000,
  );
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  if (res.value.length === 0) {
    out(style.grey("no skills yet — the agent learns them from reviewed sessions"));
    return 0;
  }
  for (const s of res.value) out(`${style.teal(s.name.padEnd(24))} ${s.description}`);
  return 0;
}

export async function configCommand(client: IRpcClient): Promise<number> {
  const res = await client.call<unknown>("config.get", {}, 30_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  out(JSON.stringify(res.value, null, 2));
  return 0;
}
