// `regent soul` / `regent about` — view or edit the agent persona (soul) and
// the user profile (about). Stored in the DB; the daemon owns the store, so the
// CLI reads/writes via persona.*. Editing happens in your editor, pre-filled
// with the current text (so you edit in place), or one-shot via `set`/`clear`.
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { out, printError } from "@app/cli/runtime.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";

type Kind = "soul" | "about";

const LABEL: Record<Kind, string> = {
  soul: "soul (agent persona)",
  about: "about-you (your profile)",
};

const HELP = (k: Kind): string =>
  `edit it:  regent ${k} edit   ·   replace:  regent ${k} set "<text>"   ·   empty:  regent ${k} clear`;

/** `regent persona` — view the whole persona (soul) + user profile (about). */
export async function personaShowAll(client: IRpcClient): Promise<number> {
  for (const kind of ["soul", "about"] as const) {
    const res = await client.call<{ content: string }>("persona.get", { key: kind }, 15_000);
    if (!res.ok) {
      printError(res.error.message);
      return 1;
    }
    out(style.heading(LABEL[kind]));
    out(res.value.content.trim() || style.grey("(empty)"));
    out("");
  }
  out(style.grey(`${HELP("soul")}   (or /soul, /about in chat)`));
  return 0;
}

export async function personaCommand(
  client: IRpcClient,
  kind: Kind,
  args: string[],
): Promise<number> {
  const sub = args[0] ?? "show";

  if (sub === "set") {
    const text = args.slice(1).join(" ").trim();
    if (!text) {
      printError(`usage: regent ${kind} set "<text>"`);
      return 1;
    }
    return save(client, kind, text);
  }

  if (sub === "clear" || sub === "delete") {
    return save(client, kind, "");
  }

  if (sub === "edit") {
    return editInEditor(client, kind);
  }

  // show (default)
  const res = await client.call<{ content: string }>("persona.get", { key: kind }, 15_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  const content = res.value.content.trim();
  out(style.heading(LABEL[kind]));
  out(content || style.grey("(empty)"));
  out(style.grey(`\n  ${HELP(kind)}`));
  return 0;
}

/** Open the current text in $EDITOR (pre-filled), then save what comes back. */
async function editInEditor(client: IRpcClient, kind: Kind): Promise<number> {
  if (!process.stdin.isTTY) {
    printError(`\`regent ${kind} edit\` needs a terminal (it opens an editor).`);
    out(style.grey(`  Use anywhere (incl. chat):  regent ${kind} set "<text>"`));
    return 1;
  }
  const cur = await client.call<{ content: string }>("persona.get", { key: kind }, 15_000);
  if (!cur.ok) {
    printError(cur.error.message);
    return 1;
  }
  const before = cur.value.content.trim() || template(kind);
  const file = join(mkdtempSync(join(tmpdir(), "regent-persona-")), `${kind}.md`);
  writeFileSync(file, before);

  const editor = editorCommand();
  out(
    style.grey(
      `  Opening ${style.value(editor)} — edit the text, ${style.bold("save")}, then close to apply.`,
    ),
  );
  // shell:true resolves Windows .cmd shims (e.g. VS Code's `code`); the path is quoted.
  const r = spawnSync(`${editor} "${file}"`, { stdio: "inherit", shell: true });
  if (r.error || (typeof r.status === "number" && r.status !== 0)) {
    printError(`couldn't run editor (${editor}).`);
    out(style.grey(`  Set a blocking editor:  $EDITOR="code --wait"  (or nano/vim)`));
    out(style.grey(`  Or replace directly:    regent ${kind} set "<text>"`));
    return 1;
  }
  const after = readFileSync(file, "utf8").trim();
  if (after === before.trim()) {
    out(
      style.grey(
        "no changes detected — nothing saved. (Did you save in the editor before closing?)",
      ),
    );
    return 0;
  }
  return save(client, kind, after);
}

/** Prefer a known-blocking editor: $EDITOR, else VS Code `--wait` on Windows
 *  (Win11's Store Notepad often returns before you save), else nano. */
function editorCommand(): string {
  const env = (process.env.EDITOR || process.env.VISUAL || "").trim();
  if (env) return env;
  return process.platform === "win32" ? "code --wait" : "nano";
}

function template(kind: Kind): string {
  return kind === "soul"
    ? "# Soul — how the agent should be\n\nName, tone, values, style.\ne.g. “Your name is Jepitot. Be concise and a little witty. No emojis.”\n"
    : "# About me\n\nYour name, role, the projects you work on, and how you like to be helped.\n";
}

async function save(client: IRpcClient, kind: Kind, content: string): Promise<number> {
  const res = await client.call("persona.set", { key: kind, content }, 15_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  out(
    `${style.pass("✓")} ${LABEL[kind]} ${content ? "saved" : "cleared"} — applies on your next chat / \`/new\``,
  );
  return 0;
}
