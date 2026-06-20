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
  gateway: "setup | start | stop | status the chat gateway",
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

export function printHelp(): number {
  const lines: string[] = [
    `${style.bold("regent")} — ${BRAND.tagline}`,
    "",
    style.heading("Usage"),
    "  regent [chat]            interactive chat (default)",
    "  regent <command> [args]",
    "",
    style.heading("Commands"),
  ];
  for (const [group, names] of Object.entries(CLI_COMMAND_GROUPS)) {
    lines.push(style.teal(`  ${group}`));
    for (const name of names) {
      lines.push(`    ${name.padEnd(10)} ${style.grey(COMMAND_HELP[name] ?? "")}`);
    }
  }
  lines.push(
    "",
    style.grey("In chat: /help · /new · /stop · /approve · /deny · /quit"),
    style.grey("Global:  -p, --profile <name>  isolate state under a profile"),
  );
  out(lines.join("\n"));
  return 0;
}
