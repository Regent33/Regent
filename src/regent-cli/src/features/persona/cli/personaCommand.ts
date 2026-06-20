// `regent soul` / `regent about` — view or edit the agent persona (soul.md) and
// the user profile (about-you.md). Stored in the DB (not plaintext files) for
// security; the daemon owns the store, so the CLI reads/writes via persona.*.
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

function template(kind: Kind): string {
  return kind === "soul"
    ? "# Soul — how the agent should be\n\nName, tone, values, style.\ne.g. “Your name is Jepitot. Be concise and a little witty. No emojis.”\n"
    : "# About me\n\nYour name, role, the projects you work on, and how you like to be helped.\n";
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
    const res = await client.call("persona.set", { key: kind, content: text }, 15_000);
    if (!res.ok) {
      printError(res.error.message);
      return 1;
    }
    out(`${style.pass("✓")} ${LABEL[kind]} updated — applies on your next chat / \`/new\``);
    return 0;
  }

  if (sub === "edit") {
    const cur = await client.call<{ content: string }>("persona.get", { key: kind }, 15_000);
    if (!cur.ok) {
      printError(cur.error.message);
      return 1;
    }
    const file = join(mkdtempSync(join(tmpdir(), "regent-persona-")), `${kind}.md`);
    writeFileSync(file, cur.value.content.trim() || template(kind));
    const editor =
      process.env.EDITOR ||
      process.env.VISUAL ||
      (process.platform === "win32" ? "notepad" : "nano");
    const r = spawnSync(editor, [file], { stdio: "inherit", shell: true });
    if (r.error) {
      printError(`could not open editor (${editor}): ${r.error.message}`);
      return 1;
    }
    const save = await client.call(
      "persona.set",
      { key: kind, content: readFileSync(file, "utf8") },
      15_000,
    );
    if (!save.ok) {
      printError(save.error.message);
      return 1;
    }
    out(style.grey(`saved — applies on your next chat / \`/new\``));
    return 0;
  }

  // show (default)
  const res = await client.call<{ content: string }>("persona.get", { key: kind }, 15_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  const content = res.value.content.trim();
  if (!content) {
    out(style.grey(`${LABEL[kind]}: empty`));
    out(style.grey(`  set it:  regent ${kind} set "<text>"   ·   edit it:  regent ${kind} edit`));
    return 0;
  }
  out(style.heading(LABEL[kind]));
  out(content);
  return 0;
}
