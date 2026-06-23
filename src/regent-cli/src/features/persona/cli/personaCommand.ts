// `regent soul` / `regent about` — view or edit the agent persona (soul) and
// the user profile (about). The profile is split into five stable facets
// (identity · preferences · habits · constraints · goals); transient/world
// facts belong in `memory`, not here. Stored in the DB; the daemon owns the
// store, so the CLI reads/writes via persona.* (keys: soul · about · about.<facet>).
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { out, printError } from "@app/cli/runtime.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";

type Kind = "soul" | "about";

// Slug → heading. Must match regent-store ABOUT_SECTIONS.
const SECTIONS: ReadonlyArray<readonly [string, string]> = [
  ["identity", "Identity"],
  ["preferences", "Preferences"],
  ["habits", "Habits"],
  ["constraints", "Constraints"],
  ["goals", "Goals"],
];
const isSection = (s: string | undefined): boolean => SECTIONS.some(([slug]) => slug === s);

const LABEL: Record<Kind, string> = {
  soul: "soul (agent persona)",
  about: "about-you (your profile)",
};

const HELP = (k: Kind): string =>
  k === "about"
    ? 'facets: identity · preferences · habits · constraints · goals   —   regent about <facet> <set|add|edit|clear> "<text>"'
    : 'verbs: regent soul <set|add|edit|clear> "<text>"';

async function getKey(client: IRpcClient, key: string): Promise<string | null> {
  const res = await client.call<{ content: string }>("persona.get", { key }, 15_000);
  if (!res.ok) {
    printError(res.error.message);
    return null;
  }
  return res.value.content;
}

/** `regent persona` — view the whole persona (soul) + user profile (about). */
export async function personaShowAll(client: IRpcClient): Promise<number> {
  const soul = await getKey(client, "soul");
  if (soul === null) return 1;
  out(style.heading(LABEL.soul));
  out(soul.trim() || style.grey("(empty)"));
  out("");
  if ((await showProfile(client)) !== 0) return 1;
  out(style.grey(`\n${HELP("soul")}   (or /soul, /about in chat)`));
  return 0;
}

/** Print the full `about` profile: legacy note (if any) + the five facets. */
async function showProfile(client: IRpcClient): Promise<number> {
  out(style.heading(LABEL.about));
  const legacy = await getKey(client, "about");
  if (legacy === null) return 1;
  if (legacy.trim()) out(legacy.trim());
  let any = legacy.trim().length > 0;
  for (const [slug, heading] of SECTIONS) {
    const v = await getKey(client, `about.${slug}`);
    if (v === null) return 1;
    if (v.trim()) {
      out(`${style.teal(`  ${heading}`)}`);
      out(`    ${v.trim().replace(/\n/g, "\n    ")}`);
      any = true;
    }
  }
  if (!any) out(style.grey("(empty)"));
  return 0;
}

export async function personaCommand(
  client: IRpcClient,
  kind: Kind,
  args: string[],
): Promise<number> {
  // `regent about <facet> ...` edits one profile facet; bare `about` shows all.
  if (kind === "about") {
    if (isSection(args[0])) {
      const [slug, ...rest] = args;
      const heading = SECTIONS.find(([s]) => s === slug)?.[1] ?? slug;
      return keyAction(client, `about.${slug}`, `about — ${heading}`, rest);
    }
    if (args.length === 0 || args[0] === "show") {
      const code = await showProfile(client);
      if (code === 0) out(style.grey(`\n  ${HELP("about")}`));
      return code;
    }
    // back-compat: `about set|edit|clear` act on the legacy general note.
    if (["set", "clear", "delete", "edit"].includes(args[0] ?? "")) {
      return keyAction(client, "about", LABEL.about, args);
    }
    printError(`unknown profile facet '${args[0]}'`);
    out(style.grey("  facets: identity · preferences · habits · constraints · goals"));
    return 1;
  }
  return keyAction(client, "soul", LABEL.soul, args);
}

/** show | set | add | edit | clear on a single persona key. */
async function keyAction(
  client: IRpcClient,
  key: string,
  label: string,
  args: string[],
): Promise<number> {
  const sub = args[0] ?? "show";
  const cmd = key.replace(".", " "); // e.g. "about identity"

  if (sub === "set" || sub === "add" || sub === "append") {
    const text = args.slice(1).join(" ").trim();
    if (!text) {
      printError(`usage: regent ${cmd} ${sub === "set" ? "set" : "add"} "<text>"`);
      return 1;
    }
    // `set` replaces; `add`/`append` keeps existing lines and adds one.
    if (sub === "set") return save(client, key, label, text);
    const cur = await getKey(client, key);
    if (cur === null) return 1;
    const next = cur.trim() ? `${cur.trim()}\n${text}` : text;
    return save(client, key, label, next);
  }
  if (sub === "clear" || sub === "delete") return save(client, key, label, "");
  if (sub === "edit") return editInEditor(client, key, label);

  // show (default)
  const content = await getKey(client, key);
  if (content === null) return 1;
  out(style.heading(label));
  out(content.trim() || style.grey("(empty)"));
  out(style.grey(`  set · add · edit · clear   —   regent ${cmd} set "<text>"`));
  return 0;
}

/** Open the current text in $EDITOR (pre-filled), then save what comes back. */
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
  // shell:true resolves Windows .cmd shims (e.g. VS Code's `code`); the path is quoted.
  const r = spawnSync(`${editor} "${file}"`, { stdio: "inherit", shell: true });
  if (r.error || (typeof r.status === "number" && r.status !== 0)) {
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

/** Prefer a known-blocking editor: $EDITOR, else VS Code `--wait` on Windows
 *  (Win11's Store Notepad often returns before you save), else nano. */
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
