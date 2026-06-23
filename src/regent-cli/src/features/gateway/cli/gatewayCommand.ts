// `regent gateway setup|start|stop|status` — manage the long-running
// `regent-gateway` process. The gateway is a separate binary (no IPC to the
// daemon), so the CLI manages it as a process: a PID file under $REGENT_HOME,
// secrets in $REGENT_HOME/.env, logs to $REGENT_HOME/logs/gateway.log.
import { type ChildProcess, spawn } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  openSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { parseFlags } from "@app/cli/args.ts";
import { out, printError } from "@app/cli/runtime.ts";
import { locateBinary, regentHome } from "@shared/infrastructure/daemon/locate.ts";
import { style } from "@shared/ui/style.ts";
import YAML from "yaml";

const pidPath = (home: string): string => join(home, "gateway.pid");

function isAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

function readPid(home: string): number | null {
  try {
    const pid = Number.parseInt(readFileSync(pidPath(home), "utf8").trim(), 10);
    return Number.isFinite(pid) ? pid : null;
  } catch {
    return null;
  }
}

// Merge $REGENT_HOME/.env into the child env (so REGENT_TELEGRAM_TOKEN etc.
// reach the gateway); the real environment always wins.
function gatewayEnv(home: string): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = { ...process.env, REGENT_HOME: home };
  try {
    for (const raw of readFileSync(join(home, ".env"), "utf8").split("\n")) {
      const line = raw.trim();
      if (!line || line.startsWith("#")) continue;
      const eq = line.indexOf("=");
      if (eq <= 0) continue;
      const key = line.slice(0, eq).trim();
      if (env[key] === undefined)
        env[key] = line
          .slice(eq + 1)
          .trim()
          .replace(/^"|"$/g, "");
    }
  } catch {
    // no .env — fine
  }
  // The gateway needs the model/provider/endpoint, which live in config.yaml
  // (not .env) — surface them as REGENT_MODEL/PROVIDER/BASE_URL so the gateway
  // doesn't fatal with "REGENT_MODEL not set". The real env still wins.
  try {
    const cfg = YAML.parse(readFileSync(join(home, "config.yaml"), "utf8")) as {
      model?: { provider?: string; default?: string; base_url?: string };
    } | null;
    const model = cfg?.model;
    if (model?.default && env.REGENT_MODEL === undefined) env.REGENT_MODEL = model.default;
    if (model?.provider && env.REGENT_PROVIDER === undefined) env.REGENT_PROVIDER = model.provider;
    if (model?.base_url && env.REGENT_BASE_URL === undefined) env.REGENT_BASE_URL = model.base_url;
  } catch {
    // no / invalid config.yaml — the start-time check reports what's missing
  }
  env.REGENT_NOW = new Date().toLocaleString(); // wall-clock for date/time answers
  return env;
}

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

function gatewayStatus(home: string): number {
  const pid = readPid(home);
  if (pid !== null && isAlive(pid)) {
    out(`${style.teal("●")} gateway running (pid ${pid})`);
  } else {
    out(`${style.grey("○")} gateway not running`);
    if (pid !== null) rmSync(pidPath(home), { force: true }); // clean a stale pid
  }
  return 0;
}

function gatewayStart(home: string): number {
  const existing = readPid(home);
  if (existing !== null && isAlive(existing)) {
    out(style.grey(`gateway already running (pid ${existing})`));
    return 0;
  }
  const located = locateBinary("regent-gateway", "REGENT_GATEWAY_PATH");
  if (!located.ok) {
    printError(located.error.message);
    return 1;
  }
  // Validate the gateway's required env up-front — otherwise it spawns, fatals
  // immediately ("REGENT_MODEL not set"), and `status` confusingly shows "not
  // running". Tell the user exactly what to set instead.
  const env = gatewayEnv(home);
  const missing = (
    [
      ["REGENT_TELEGRAM_TOKEN", "regent gateway setup <telegram-token>"],
      ["REGENT_API_KEY", "regent setup  (provider API key)"],
      ["REGENT_MODEL", "regent setup --model <id>  (writes config.yaml)"],
    ] as const
  ).filter(([k]) => !env[k]);
  if (missing.length > 0) {
    printError("gateway can't start — missing configuration:");
    for (const [k, how] of missing) out(`  ${style.fail("✗")} ${k.padEnd(22)} set via: ${how}`);
    return 1;
  }
  mkdirSync(join(home, "logs"), { recursive: true });
  const log = openSync(join(home, "logs", "gateway.log"), "a");
  let child: ChildProcess;
  try {
    child = spawn(located.value, [], {
      detached: true,
      stdio: ["ignore", log, log],
      env,
    });
  } catch (e) {
    printError(`spawn gateway: ${e instanceof Error ? e.message : String(e)}`);
    return 1;
  }
  if (child.pid === undefined) {
    printError("gateway did not start");
    return 1;
  }
  writeFileSync(pidPath(home), String(child.pid));
  child.unref();
  out(
    `gateway started (pid ${style.teal(String(child.pid))}) — logs: ${join(home, "logs", "gateway.log")}`,
  );
  return 0;
}

function gatewayStop(home: string): number {
  const pid = readPid(home);
  if (pid === null || !isAlive(pid)) {
    out(style.grey("gateway not running"));
    rmSync(pidPath(home), { force: true });
    return 0;
  }
  try {
    process.kill(pid);
  } catch (e) {
    printError(`stop gateway (pid ${pid}): ${e instanceof Error ? e.message : String(e)}`);
    return 1;
  }
  rmSync(pidPath(home), { force: true });
  out(`gateway stopped (pid ${pid})`);
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
