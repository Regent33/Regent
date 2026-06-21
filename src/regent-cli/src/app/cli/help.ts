import { out } from "@app/cli/runtime.ts";
// `regent version` and `regent help`.
import { BRAND } from "@app/config/brand.ts";
import { CLI_COMMAND_GROUPS } from "@app/config/commands.ts";
import { style } from "@shared/ui/style.ts";

export const CLI_VERSION = BRAND.version;

// One-line usage/description per command. Help is generated from
// CLI_COMMAND_GROUPS (the single source of truth) + this map, so adding a
// command there makes it appear here automatically — a missing description just
// renders blank, never an omission.
const COMMAND_HELP: Record<string, string> = {
  chat: "interactive chat with the agent (default)",
  sessions: "list | search | resume past sessions",
  memory: "pending | approve | reject staged memory writes",
  status: "daemon health / model / cron snapshot",
  kanban: "list | create | show | assign | block | unblock | complete",
  model: "show · list · set <id>",
  skills: "list · view · create · opt-out",
  tools: "list · enable | disable <tool>",
  config: "show · set <key> <value>",
  profile: "list · create · delete profile homes",
  setup: "first-time configuration (provider, model, key)",
  keys: "manage provider API keys (list · set · rm) in .env",
  persona: "view the whole persona + profile (soul · about)",
  soul: "view/edit the agent persona (show · edit · set)",
  about: "view/edit your user profile (show · edit · set)",
  gateway: "setup <token> | start | stop | status | enable | disable",
  auth: "status · revoke <user>",
  cron: "list · add · remove scheduled jobs",
  logs: "show the daemon log (-f to follow)",
  doctor: "check the installation",
  security: "audit perms / secrets",
  insights: "usage rollup (turns, tokens, api calls)",
  debug: "redacted bug-report bundle",
  mcp: "serve Regent's tools over MCP (stdio)",
  version: "print the CLI version",
};

export function printVersion(): number {
  out(`regent ${CLI_VERSION}`);
  return 0;
}

const id = <T>(s: T) => s;

// The full help text, built from CLI_COMMAND_GROUPS + COMMAND_HELP (single
// source). `color` paints it for the shell (`regent help`); plain text is used
// for the in-chat `/help` note (embedded ANSI would corrupt Ink's layout).
function helpLines(color: boolean): string[] {
  const head = color ? style.heading : id<string>;
  const grp = color ? style.teal : id<string>;
  const dim = color ? style.grey : id<string>;
  const bold = color ? style.bold : id<string>;
  const lines: string[] = [
    `${bold("regent")} — ${BRAND.tagline}`,
    "",
    head("Usage"),
    "  regent [chat]            interactive chat (default)",
    "  regent <command> [args]",
    "",
    `${head("Commands")}  ${dim("— run from your shell as: regent <command> [args]")}`,
  ];
  for (const [group, names] of Object.entries(CLI_COMMAND_GROUPS)) {
    lines.push(grp(`  ${group}`));
    for (const name of names) {
      lines.push(`    ${name.padEnd(10)} ${dim(COMMAND_HELP[name] ?? "")}`);
    }
  }
  lines.push(
    "",
    `${head("In chat")}  ${dim("— type these inside a session")}`,
    `  ${"/help".padEnd(10)} ${dim("show this help")}`,
    `  ${"/doctor".padEnd(10)} ${dim("check provider / model / API-key (diagnose 401s)")}`,
    `  ${"/new".padEnd(10)} ${dim("clear the transcript (also /clear)")}`,
    `  ${"/stop".padEnd(10)} ${dim("interrupt the running turn")}`,
    `  ${"/approve".padEnd(10)} ${dim("approve a pending sensitive action (also /deny)")}`,
    `  ${"/quit".padEnd(10)} ${dim("leave Regent (also /exit)")}`,
    dim("  …plus any command above, prefixed with / — e.g. /status, /kanban list, /soul, /persona"),
    "",
    dim("Global:  -p, --profile <name>  isolate state under a profile"),
  );
  return lines;
}

/** Plain-text help for the in-chat `/help` note. */
export function helpText(): string {
  return helpLines(false).join("\n");
}

export function printHelp(): number {
  out(helpLines(true).join("\n"));
  return 0;
}
