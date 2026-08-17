// `regent gateway setup|start|stop|status|enable|disable` — manage the
// long-running `regent-gateway` process. The gateway is a separate binary (no
// IPC to the deacon), so the CLI manages it as a process: a PID file under
// $REGENT_HOME (see `gatewayProcess`), secrets in $REGENT_HOME/.env, config →
// child env in `gatewayEnv`, logs to $REGENT_HOME/logs/gateway.log.
import { rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { parseFlags } from "@app/cli/args.ts";
import { out, printError } from "@app/cli/runtime.ts";
import { type ReadSecret, readStdin, secretFromStdin } from "@app/cli/secretStdin.ts";
import { regentHome } from "@shared/infrastructure/deacon/locate.ts";
import { updateDotenv } from "@shared/infrastructure/storage/dotenvFile.ts";
import { style } from "@shared/ui/style.ts";
import { gatewayStart, gatewayStatus, gatewayStop } from "./gatewayProcess.ts";

export function gatewayCommand(
  profile: string,
  args: string[],
  readSecret: ReadSecret = readStdin,
): number {
  const home = regentHome(profile);
  switch (args[0]) {
    case "status":
      return gatewayStatus(home);
    case "start":
      return gatewayStart(home);
    case "stop":
      return gatewayStop(home);
    case "setup":
      return gatewaySetup(home, args.slice(1), readSecret);
    case "enable":
      return gatewayEnable(profile);
    case "disable":
      return gatewayDisable(home);
    default:
      printError("usage: regent gateway setup|start|stop|status|enable|disable");
      return 1;
  }
}

// Windows login-startup entry that auto-starts the gateway after a reboot
// (without it, the detached gateway survives a terminal close but not a reboot).
function startupCmdPath(): string {
  return join(
    process.env.APPDATA ?? "",
    "Microsoft",
    "Windows",
    "Start Menu",
    "Programs",
    "Startup",
    "regent-gateway.cmd",
  );
}

function gatewayEnable(profile: string): number {
  if (process.platform !== "win32") {
    printError(
      "`gateway enable` currently supports Windows; on macOS/Linux use a launchd/systemd unit.",
    );
    return 1;
  }
  const prof = profile ? ` -p ${profile}` : "";
  // process.execPath is the compiled regent binary (absolute) — no PATH reliance.
  writeFileSync(startupCmdPath(), `@echo off\r\n"${process.execPath}"${prof} gateway start\r\n`);
  out(`${style.pass("✓")} gateway will auto-start at login`);
  out(style.grey(`  startup entry: ${startupCmdPath()}`));
  out(style.grey("  turn off with: regent gateway disable"));
  return 0;
}

function gatewayDisable(home: string): number {
  rmSync(startupCmdPath(), { force: true });
  out("gateway auto-start disabled (login entry removed)");
  gatewayStop(home); // also stop the running gateway, per "disable = off"
  return 0;
}

// Messaging platforms `gateway setup` can configure. `runs` = the gateway binary
// can actually run it today (Telegram); the rest are saved (ready) but selecting
// them at runtime lands with the gateway's multi-platform support.
const GW_PLATFORMS = [
  {
    id: "telegram",
    label: "Telegram",
    tokenVar: "REGENT_TELEGRAM_TOKEN",
    hint: "@BotFather",
    runs: true,
  },
  {
    id: "discord",
    label: "Discord",
    tokenVar: "REGENT_DISCORD_TOKEN",
    hint: "discord.com/developers → Bot → Token",
    runs: false,
  },
  {
    id: "whatsapp",
    label: "WhatsApp",
    tokenVar: "REGENT_WHATSAPP_TOKEN",
    hint: "Meta WhatsApp Cloud API",
    runs: false,
  },
  {
    id: "slack",
    label: "Slack",
    tokenVar: "REGENT_SLACK_TOKEN",
    hint: "api.slack.com/apps",
    runs: false,
  },
] as const;

// `regent gateway setup <platform> --token-stdin` — saves the platform's bot
// token (and for Telegram, starts it), read from a pipe.
//
// `--token <t>` and the old bare `gateway setup <token>` positional are REMOVED
// rather than deprecated-with-a-warning, deliberately: by the time this process
// could print a warning, the shell has already written the token to its history
// file and `ps`/Task Manager has already been able to read it off the command
// line. A warning would soften the message about a leak that already happened,
// and it would keep the leaking path working for every script that has one.
// `regent setup --key` and `regent keys set` refuse argv secrets the same way;
// a third convention here is the change that would really cost users. The
// refusal names the replacement, so the break is one edit, not a mystery.
function gatewaySetup(home: string, args: string[], readSecret: ReadSecret): number {
  const { values, positionals } = parseFlags(args, {
    token: { type: "string" },
    "token-stdin": { type: "boolean" },
    "allow-all": { type: "boolean" },
    "allowed-users": { type: "string" },
    "no-start": { type: "boolean" },
  });
  // First positional may name a platform; any further one was the old token.
  const named = GW_PLATFORMS.find((p) => p.id === (positionals[0] ?? "").toLowerCase());
  const plat = named ?? GW_PLATFORMS[0];
  const rest = named ? positionals.slice(1) : positionals;

  if (values.token !== undefined || rest.length > 0) {
    printError(
      `do not put secrets in command history; pipe the token to \`regent gateway setup ${plat.id} --token-stdin\``,
    );
    return 2; // EXIT.usage — the invocation itself was the unsafe part.
  }
  if (values["token-stdin"] !== true) {
    printError("usage: regent gateway setup <platform> --token-stdin");
    out(style.grey(`  platforms: ${GW_PLATFORMS.map((p) => p.id).join(", ")}`));
    out(
      style.grey(
        `  e.g. cat token.txt | regent gateway setup ${plat.id} --token-stdin   (token from ${plat.hint})`,
      ),
    );
    return 1;
  }
  let token: string;
  try {
    token = secretFromStdin(readSecret);
  } catch (error) {
    printError(`could not read the ${plat.label} token from stdin: ${String(error)}`);
    return 1;
  }

  const updates: Record<string, string> = {
    [plat.tokenVar]: token,
    REGENT_GATEWAY_PLATFORM: plat.id,
  };
  const restricted = typeof values["allowed-users"] === "string";
  if (plat.id === "telegram") {
    if (restricted) updates.REGENT_TELEGRAM_ALLOWED_USERS = values["allowed-users"] as string;
    else updates.REGENT_TELEGRAM_ALLOW_ALL = "1"; // works out of the box
  }
  upsertEnv(home, updates);
  out(`${style.pass("✓")} ${plat.label} token saved`);

  if (!plat.runs) {
    out(style.warn(`  ⚠ the gateway runs Telegram today — ${plat.label} is saved but not yet`));
    out(style.grey("    selectable at runtime (lands with multi-platform gateway support)."));
    return 0;
  }
  if (!restricted) {
    out(style.warn("  ⚠ anyone who finds your bot can message it (and spend your API key)."));
    out(
      style.grey(
        "  lock it down: … | regent gateway setup telegram --token-stdin --allowed-users <your-id>",
      ),
    );
  }
  if (values["no-start"]) {
    out(style.grey("  then start it with: regent gateway start"));
    return 0;
  }
  out(style.grey("  starting the gateway…"));
  return gatewayStart(home); // one command: save + start
}

// Upsert KEY=VALUE lines into $REGENT_HOME/.env (atomic, owner-only).
function upsertEnv(home: string, updates: Record<string, string>): void {
  updateDotenv(join(home, ".env"), updates);
}
