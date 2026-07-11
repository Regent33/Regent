// `regent gateway setup|start|stop|status` — manage the long-running
// `regent-gateway` process. The gateway is a separate binary (no IPC to the
// deacon), so the CLI manages it as a process: a PID file under $REGENT_HOME,
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
import { locateBinary, regentHome } from "@shared/infrastructure/deacon/locate.ts";
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
  // (not .env). Resolve the SAME primary→fallback chain the deacon uses so the
  // gateway routes identically and rotating keys / a changed model take effect
  // on the next start. The chosen base_url and key come from the SAME provider
  // (the old code paired config's base_url with the generic REGENT_API_KEY —
  // e.g. ollama.com + an OpenRouter key → HTTP 401). The real env still wins.
  try {
    const cfg = YAML.parse(readFileSync(join(home, "config.yaml"), "utf8")) as GatewayConfig | null;
    const chain = resolveProviderChain(cfg, env);
    const primary = chain[0];
    if (primary !== undefined) {
      env.REGENT_PROVIDER_CHAIN = JSON.stringify(chain);
      // Keep the legacy vars set (to the primary) so the start-time validation
      // passes and a non-chain gateway build still runs. NOT REGENT_BASE_URL —
      // the chain carries the correct base per link; a stale one would mislead.
      if (env.REGENT_MODEL === undefined) env.REGENT_MODEL = primary.model;
      if (env.REGENT_API_KEY === undefined) env.REGENT_API_KEY = primary.api_key;
    } else {
      // No providers map / agents_defaults — a plain single-provider setup.
      const model = cfg?.model;
      if (model?.default && env.REGENT_MODEL === undefined) env.REGENT_MODEL = model.default;
      if (model?.provider && env.REGENT_PROVIDER === undefined) env.REGENT_PROVIDER = model.provider;
      if (model?.base_url && env.REGENT_BASE_URL === undefined) env.REGENT_BASE_URL = model.base_url;
    }
  } catch {
    // no / invalid config.yaml — the start-time check reports what's missing
  }
  env.REGENT_NOW = new Date().toLocaleString(); // wall-clock for date/time answers
  return env;
}

interface ProviderSpec {
  kind?: string;
  base_url?: string;
  api_key_env?: string;
  models?: string[];
}
interface ModelRef {
  provider?: string;
  model?: string;
  key_slot?: number;
}
interface GatewayConfig {
  model?: { provider?: string; default?: string; base_url?: string };
  providers?: Record<string, ProviderSpec>;
  agents_defaults?: { primary?: ModelRef; fallbacks?: ModelRef[] };
}

// Default OpenAI-compatible base URL per provider kind — mirrors the deacon's
// provider_kind.rs. Only kinds that serve the standard /v1/chat/completions
// path (the gateway's fixed path) are listed; a provider with an explicit
// base_url in config always wins over this.
const KIND_BASE: Record<string, string> = {
  openai: "https://openrouter.ai/api",
  openrouter: "https://openrouter.ai/api",
  groq: "https://api.groq.com/openai",
  deepseek: "https://api.deepseek.com",
  together: "https://api.together.xyz",
  ollama: "http://localhost:11434",
  mistral: "https://api.mistral.ai",
  xai: "https://api.x.ai",
  moonshot: "https://api.moonshot.ai",
  dashscope: "https://dashscope-intl.aliyuncs.com/compatible-mode",
  fireworks: "https://api.fireworks.ai/inference",
  cerebras: "https://api.cerebras.ai",
  minimax: "https://api.minimax.io",
  nvidia: "https://integrate.api.nvidia.com",
};

interface ChainLink {
  base_url: string;
  api_key: string;
  model: string;
}

// Resolve the primary→fallback chain into concrete {base_url, api_key, model}
// links, in order, dropping any whose provider/base/key can't be resolved (a
// missing key just means that link is skipped, like the deacon's chain does).
function resolveProviderChain(
  cfg: GatewayConfig | null,
  env: NodeJS.ProcessEnv,
): ChainLink[] {
  const providers = cfg?.providers ?? {};
  const refs: ModelRef[] = [];
  if (cfg?.agents_defaults?.primary) refs.push(cfg.agents_defaults.primary);
  for (const fb of cfg?.agents_defaults?.fallbacks ?? []) refs.push(fb);

  const chain: ChainLink[] = [];
  const seen = new Set<string>();
  for (const ref of refs) {
    const name = ref?.provider;
    const model = ref?.model;
    if (!name || !model) continue;
    const spec = providers[name];
    if (!spec) continue;
    const base = spec.base_url ?? KIND_BASE[(spec.kind ?? name).toLowerCase()];
    // key_slot ≥ 2 reads the slotted var (<BASE>_<N>); slot 1/absent = base var.
    const keyEnv =
      spec.api_key_env && ref.key_slot && ref.key_slot >= 2
        ? `${spec.api_key_env}_${ref.key_slot}`
        : spec.api_key_env;
    const key = keyEnv ? env[keyEnv] : undefined;
    if (!base || !key) continue; // can't form this link — skip it, like the deacon
    const dedup = `${base}|${model}`;
    if (seen.has(dedup)) continue;
    seen.add(dedup);
    chain.push({ base_url: base, api_key: key, model });
  }
  return chain;
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
