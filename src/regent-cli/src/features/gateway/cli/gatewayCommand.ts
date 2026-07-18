// `regent gateway setup|start|stop|status|enable|disable` — manage the
// long-running `regent-gateway` process. The gateway is a separate binary (no
// IPC to the deacon), so the CLI manages it as a process: a PID file under
// $REGENT_HOME (see `gatewayProcess`), secrets in $REGENT_HOME/.env, config →
// child env in `gatewayEnv`, logs to $REGENT_HOME/logs/gateway.log.
import { mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { parseFlags } from "@app/cli/args.ts";
import { out, printError } from "@app/cli/runtime.ts";
import { regentHome } from "@shared/infrastructure/deacon/locate.ts";
import { style } from "@shared/ui/style.ts";
import { gatewayStart, gatewayStatus, gatewayStop } from "./gatewayProcess.ts";

export function gatewayCommand(profile: string, args: string[]): number {
  const home = regentHome(profile);
  switch (args[0]) {
    case "status":
      return gatewayStatus(home);
    case "start":
      return gatewayStart(home);
    case "stop":
      return gatewayStop(home);
    case "setup":
      return gatewaySetup(home, args.slice(1));
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

// `regent gateway setup <platform> <token>` — saves the platform's bot token (and
// for Telegram, starts it). Back-compat: a bare `gateway setup <token>` = Telegram.
function gatewaySetup(home: string, args: string[]): number {
  const { values, positionals } = parseFlags(args, {
    token: { type: "string" },
    "allow-all": { type: "boolean" },
    "allowed-users": { type: "string" },
    "no-start": { type: "boolean" },
  });
  // First positional may name a platform; otherwise it's a (Telegram) token.
  const named = GW_PLATFORMS.find((p) => p.id === (positionals[0] ?? "").toLowerCase());
  const plat = named ?? GW_PLATFORMS[0];
  const rest = named ? positionals.slice(1) : positionals;
  const token = (typeof values.token === "string" ? values.token : rest[0])?.trim();

  if (!token) {
    printError("usage: regent gateway setup <platform> <token>");
    out(style.grey(`  platforms: ${GW_PLATFORMS.map((p) => p.id).join(", ")}`));
    out(style.grey(`  e.g. regent gateway setup ${plat.id} <token>   (token from ${plat.hint})`));
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
      style.grey("  lock it down: regent gateway setup telegram <token> --allowed-users <your-id>"),
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
  const path = join(home, ".env");
  const kept: string[] = [];
  try {
    for (const line of readFileSync(path, "utf8").split("\n")) {
      const key = line.slice(0, Math.max(0, line.indexOf("="))).trim();
      if (line.trim() === "" || key in updates) continue;
      kept.push(line);
    }
  } catch {
    // no existing .env
  }
  for (const [k, v] of Object.entries(updates)) kept.push(`${k}=${v}`);
  mkdirSync(home, { recursive: true });
  const tmp = join(home, `.env.tmp.${process.pid}`);
  writeFileSync(tmp, `${kept.join("\n")}\n`, { mode: 0o600 });
  renameSync(tmp, path);
}
