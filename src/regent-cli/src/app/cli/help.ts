import { out } from "@app/cli/runtime.ts";
// `regent version` and `regent help`.
import { BRAND } from "@app/config/brand.ts";
import { style } from "@shared/ui/style.ts";

export const CLI_VERSION = BRAND.version;

export function printVersion(): number {
  out(`regent ${CLI_VERSION}`);
  return 0;
}

export function printHelp(): number {
  out(
    [
      `${style.bold("regent")} — ${BRAND.tagline}`,
      "",
      style.heading("Usage"),
      "  regent [chat]                  interactive chat (default)",
      "  regent <command> [args]",
      "",
      style.heading("Commands"),
      "  chat                           interactive chat with the agent",
      "  status                         daemon health / model / cron snapshot",
      "  model [list | set <id>]        show / list / switch the model",
      "  skills [view|create|opt-out]   list / author skills",
      "  tools [enable|disable <t>]     list / toggle tools",
      "  config [set <key> <val>]       show / edit the config",
      "  sessions list|search|resume    browse or resume past sessions",
      "  memory pending|approve|reject  review staged memory writes",
      "  cron list | add | remove       manage scheduled jobs",
      "  kanban list|create|show|…      shared work board (assign/block/complete)",
      "  gateway setup|start|stop|status  run the chat gateway",
      "  auth status|revoke <user>      gateway pairing/authorization",
      "  profile list|create|delete     manage profile homes",
      "  mcp serve                      expose Regent's tools over MCP (stdio)",
      "  logs [-f]                      show the daemon log",
      "  doctor                         check the installation",
      "  security audit                 lint perms / secrets",
      "  insights                       usage rollup (turns, tokens, api calls)",
      "  debug                          redacted bug-report bundle",
      "  setup                          first-time configuration",
      "  version                        print the CLI version",
      "",
      style.grey("Global: -p, --profile <name>  isolate state under a profile"),
    ].join("\n"),
  );
  return 0;
}
