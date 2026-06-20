// The command router (cobra-equivalent): parse the global profile flag, then
// dispatch the first positional to its handler. Bare `regent` / `regent chat`
// open the interactive TUI; everything else is a one-shot command.
import { extractProfile } from "@app/cli/args.ts";
import { printHelp, printVersion } from "@app/cli/help.ts";
import { runChat } from "@app/cli/runChat.tsx";
import { out, printError, withClient } from "@app/cli/runtime.ts";
import { cronCommand } from "@features/cron/cli/cronCommand.ts";
import { debugCommand } from "@features/debug/cli/debugCommand.ts";
import { doctorCommand } from "@features/doctor/cli/doctorCommand.ts";
import { authCommand } from "@features/gateway/cli/authCommand.ts";
import { gatewayCommand } from "@features/gateway/cli/gatewayCommand.ts";
import { insightsCommand } from "@features/insights/cli/insightsCommand.ts";
import { configSetCommand } from "@features/inspect/cli/configSetCommand.ts";
import {
  configCommand,
  modelCommand,
  skillsCommand,
} from "@features/inspect/cli/inspectCommands.ts";
import { kanbanCommand } from "@features/kanban/cli/kanbanCommand.ts";
import { logsCommand } from "@features/logs/cli/logsCommand.ts";
import { mcpCommand } from "@features/mcp/cli/mcpCommand.ts";
import { memoryCommand } from "@features/memory/cli/memoryCommand.ts";
import { personaCommand } from "@features/persona/cli/personaCommand.ts";
import { profileCommand } from "@features/profile/cli/profileCommand.ts";
import { securityCommand } from "@features/security/cli/securityCommand.ts";
import { sessionsCommand } from "@features/sessions/cli/sessionsCommand.ts";
import { setupCommand } from "@features/setup/cli/setupCommand.ts";
import { statusCommand } from "@features/status/cli/statusCommand.ts";
import { toolsListCommand, toolsSetCommand } from "@features/tools/cli/toolsCommand.ts";

export async function runCli(argv: readonly string[]): Promise<number> {
  const { profile, rest } = extractProfile(argv);
  const [command = "", ...args] = rest;

  switch (command) {
    case "":
    case "chat":
      return runChat(profile);
    case "model":
      return withClient(profile, (c) => modelCommand(c, args));
    case "skills":
      return withClient(profile, (c) => skillsCommand(c, args));
    case "config":
      if (args[0] === "set") return configSetCommand(profile, args.slice(1));
      return withClient(profile, (c) => configCommand(c));
    case "sessions":
      // `sessions resume <id>` opens the chat surface on an existing session.
      if (args[0] === "resume") return runChat(profile, args[1]);
      return withClient(profile, (c) => sessionsCommand(c, args));
    case "cron":
      return withClient(profile, (c) => cronCommand(c, args));
    case "memory":
      return withClient(profile, (c) => memoryCommand(c, args));
    case "tools":
      if (args[0] === "enable" || args[0] === "disable") {
        return toolsSetCommand(profile, args[0], args[1]);
      }
      return withClient(profile, (c) => toolsListCommand(c));
    case "gateway":
      return gatewayCommand(profile, args);
    case "auth":
      return authCommand(profile, args);
    case "profile":
      return profileCommand(args);
    case "soul":
      return personaCommand(profile, "soul", args);
    case "about":
      return personaCommand(profile, "about", args);
    case "status":
      return withClient(profile, (c) => statusCommand(c));
    case "insights":
      return withClient(profile, (c) => insightsCommand(c));
    case "kanban":
      return withClient(profile, (c) => kanbanCommand(c, args));
    case "debug":
      return debugCommand(profile);
    case "logs":
      return logsCommand(profile, args);
    case "doctor":
      return doctorCommand(profile);
    case "security":
      return securityCommand(profile, args);
    case "mcp":
      return mcpCommand(profile, args);
    case "setup":
      return setupCommand(profile, args);
    case "version":
    case "--version":
    case "-v":
      return printVersion();
    case "help":
    case "--help":
    case "-h":
      return printHelp();
    default:
      printError(`unknown command: ${command}`);
      out("");
      printHelp();
      return 1;
  }
}
