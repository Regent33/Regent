// `regent soul` / `regent about` — view or edit the persona files the daemon
// injects into the system prompt: soul.md (how Regent should be) and
// about-you.md (who the user is), under $REGENT_HOME. CLI-local (host files).
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { out, printError } from "@app/cli/runtime.ts";
import { regentHome } from "@shared/infrastructure/daemon/locate.ts";
import { style } from "@shared/ui/style.ts";

type Kind = "soul" | "about";

const FILE: Record<Kind, string> = { soul: "soul.md", about: "about-you.md" };
const LABEL: Record<Kind, string> = {
  soul: "soul (agent persona)",
  about: "about-you (your profile)",
};

function template(kind: Kind): string {
  return kind === "soul"
    ? "# Soul — how Regent should be\n\nDescribe the persona, tone, values, and style you want.\ne.g. “Be concise and a little witty. Prefer bullet points. No emojis.”\n"
    : "# About me — what Regent should know about you\n\nYour name, role, the projects you work on, and how you like to be helped.\n";
}

export function personaCommand(profile: string, kind: Kind, args: string[]): number {
  const home = regentHome(profile);
  const file = join(home, FILE[kind]);
  const sub = args[0] ?? "show";

  if (sub === "path") {
    out(file);
    return 0;
  }

  if (sub === "edit") {
    mkdirSync(home, { recursive: true });
    if (!existsSync(file)) writeFileSync(file, template(kind));
    const editor =
      process.env.EDITOR ||
      process.env.VISUAL ||
      (process.platform === "win32" ? "notepad" : "nano");
    const res = spawnSync(editor, [file], { stdio: "inherit", shell: true });
    if (res.error) {
      printError(`could not open editor (${editor}): ${res.error.message}`);
      out(style.grey(`edit it directly: ${file}`));
      return 1;
    }
    out(style.grey(`saved — applies on your next \`regent chat\``));
    return 0;
  }

  if (sub === "set") {
    const text = args.slice(1).join(" ").trim();
    if (!text) {
      printError(`usage: regent ${kind} set "<text>"`);
      return 1;
    }
    mkdirSync(home, { recursive: true });
    writeFileSync(file, `${text}\n`);
    out(`${style.pass("✓")} ${LABEL[kind]} updated — applies on your next \`regent chat\``);
    return 0;
  }

  // show (default)
  if (!existsSync(file)) {
    out(style.grey(`${LABEL[kind]}: empty`));
    out(style.grey(`  edit in your editor:  regent ${kind} edit`));
    out(style.grey(`  or set inline:        regent ${kind} set "<text>"`));
    return 0;
  }
  out(style.heading(LABEL[kind]));
  out(readFileSync(file, "utf8").trimEnd());
  out(style.grey(`\n(edit: regent ${kind} edit · file: ${file})`));
  return 0;
}
