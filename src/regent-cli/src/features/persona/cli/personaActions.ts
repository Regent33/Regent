import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { out, printError } from "@app/cli/runtime.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";

export async function getKey(client: IRpcClient, key: string): Promise<string | null> {
  const res = await client.call<{ content: string }>("persona.get", { key }, 15_000);
  if (!res.ok) {
    printError(res.error.message);
    return null;
  }
  return res.value.content;
}

/** show | set | add | edit | clear on a single persona key. */
export async function keyAction(
  client: IRpcClient,
  key: string,
  label: string,
  args: string[],
): Promise<number> {
  const sub = args[0] ?? "show";
  const cmd = key.replace(".", " ");

  if (sub === "set" || sub === "add" || sub === "append") {
    const text = args.slice(1).join(" ").trim();
    if (!text) {
      printError(`usage: regent ${cmd} ${sub === "set" ? "set" : "add"} "<text>"`);
      return 1;
    }
    if (sub === "set") return save(client, key, label, text);
    const cur = await getKey(client, key);
    if (cur === null) return 1;
    const next = cur.trim() ? `${cur.trim()}\n${text}` : text;
    return save(client, key, label, next);
  }
  if (sub === "clear" || sub === "delete") return save(client, key, label, "");
  if (sub === "edit") return editInEditor(client, key, label);

  const content = await getKey(client, key);
  if (content === null) return 1;
  out(style.heading(label));
  out(content.trim() || style.grey("(empty)"));
  out(style.grey(`  set · add · edit · clear   —   regent ${cmd} set "<text>"`));
  return 0;
}

async function editInEditor(client: IRpcClient, key: string, label: string): Promise<number> {
  if (!process.stdin.isTTY) {
    printError(`\`regent ${key.replace(".", " ")} edit\` needs a terminal (it opens an editor).`);
    out(style.grey(`  Use anywhere (incl. chat):  regent ${key.replace(".", " ")} set "<text>"`));
    return 1;
  }
  const cur = await getKey(client, key);
  if (cur === null) return 1;
  const before = cur.trim() || template(key);
  const file = join(mkdtempSync(join(tmpdir(), "regent-persona-")), `${key.replace(".", "-")}.md`);
  writeFileSync(file, before);

  const editor = editorCommand();
  out(
    style.grey(
      `  Opening ${style.value(editor)} — edit the text, ${style.bold("save")}, then close to apply.`,
    ),
  );
  const result = spawnSync(`${editor} "${file}"`, { stdio: "inherit", shell: true });
  if (result.error || (typeof result.status === "number" && result.status !== 0)) {
    printError(`couldn't run editor (${editor}).`);
    out(style.grey(`  Set a blocking editor:  $EDITOR="code --wait"  (or nano/vim)`));
    out(style.grey(`  Or replace directly:    regent ${key.replace(".", " ")} set "<text>"`));
    return 1;
  }
  const after = readFileSync(file, "utf8").trim();
  if (after === before.trim()) {
    out(style.grey("no changes detected — nothing saved. (Did you save before closing?)"));
    return 0;
  }
  return save(client, key, label, after);
}

function editorCommand(): string {
  const env = (process.env.EDITOR || process.env.VISUAL || "").trim();
  if (env) return env;
  return process.platform === "win32" ? "code --wait" : "nano";
}

function template(key: string): string {
  if (key === "soul") {
    return "# Soul — how the agent should be\n\nName, tone, values, style.\ne.g. “Your name is Jepitot. Be concise and a little witty. No emojis.”\n";
  }
  if (key.startsWith("about.")) {
    return `# ${key.slice("about.".length)} — one facet of your profile\n\n`;
  }
  return "# About me\n\nYour name, role, the projects you work on, and how you like to be helped.\n";
}

async function save(
  client: IRpcClient,
  key: string,
  label: string,
  content: string,
): Promise<number> {
  const res = await client.call("persona.set", { key, content }, 15_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  out(
    `${style.pass("✓")} ${label} ${content ? "saved" : "cleared"} — applies on your next chat / \`/new\``,
  );
  return 0;
}
