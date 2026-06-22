// `regent voice setup|status|models|enable|disable` — turn on and inspect the
// voice (ASR/TTS) stack. Off by default; `setup` is the one intuitive command
// that picks a provider, saves the key + config, and enables it. status/models
// read the daemon (voice.status/voice.models); setup/enable/disable edit
// $REGENT_HOME/config.yaml + .env directly (the daemon reloads config each run).
import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { parseFlags } from "@app/cli/args.ts";
import { out, printError, withClient } from "@app/cli/runtime.ts";
import { regentHome } from "@shared/infrastructure/daemon/locate.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";
import YAML from "yaml";

// OpenAI-compatible providers. `local` is the default — Qwen3 served by a
// localhost server (no key); qwen/groq/openai are hosted alternatives. Must
// stay in sync with the daemon's speech_factory resolve_base/resolve_key.
const PROVIDERS = ["local", "qwen", "groq", "openai"] as const;
type Provider = (typeof PROVIDERS)[number];

/** Env var holding a provider's API key (matches speech_factory::resolve_key). */
export function providerKeyVar(provider: string): string | null {
  switch (provider) {
    case "groq":
      return "GROQ_API_KEY";
    case "openai":
      return "OPENAI_API_KEY";
    case "qwen":
    case "dashscope":
      return "DASHSCOPE_API_KEY";
    default:
      return null;
  }
}

/** Sensible default ASR/TTS model ids per provider; Qwen is the headline. */
export function defaultModels(provider: string): { asr: string; tts: string } {
  switch (provider) {
    case "groq":
      return { asr: "whisper-large-v3-turbo", tts: "" };
    case "openai":
      return { asr: "whisper-1", tts: "gpt-4o-mini-tts" };
    default:
      return { asr: "qwen3-asr", tts: "qwen3-tts" };
  }
}

/** Merge speech settings into a parsed config.yaml doc, preserving other keys. */
export function applySpeechConfig(
  doc: Record<string, unknown>,
  opts: {
    provider: string;
    asrModel: string;
    ttsModel: string;
    baseUrl: string;
    enabled: boolean;
  },
): void {
  const speech = (
    typeof doc.speech === "object" && doc.speech !== null ? doc.speech : {}
  ) as Record<string, unknown>;
  speech.enabled = opts.enabled;
  speech.asr = { provider: opts.provider, model: opts.asrModel, base_url: opts.baseUrl };
  speech.tts = { provider: opts.provider, model: opts.ttsModel, base_url: opts.baseUrl };
  doc.speech = speech;
}

export async function voiceCommand(profile: string, args: string[]): Promise<number> {
  switch (args[0]) {
    case "setup":
      return voiceSetup(profile, args.slice(1));
    case "enable":
      return setEnabled(profile, true);
    case "disable":
      return setEnabled(profile, false);
    case "status":
      return withClient(profile, voiceStatus);
    case "models":
      return withClient(profile, voiceModels);
    default:
      printError("usage: regent voice setup|status|models|enable|disable");
      out(style.grey("  start here: regent voice setup"));
      return 1;
  }
}

function voiceSetup(profile: string, args: string[]): number {
  const { values } = parseFlags(args, {
    provider: { type: "string" },
    "asr-model": { type: "string" },
    "tts-model": { type: "string" },
    "base-url": { type: "string" },
    key: { type: "string" },
    "no-enable": { type: "boolean" },
  });
  const home = regentHome(profile);

  let provider = str(values.provider);
  if (!provider) {
    out(
      `  ${style.grey(`providers: ${PROVIDERS.join(", ")} (local = Qwen3 on a localhost server)`)}`,
    );
    provider = ask("Provider", "local");
  }
  if (!PROVIDERS.includes(provider as Provider)) {
    printError(`unknown provider "${provider}" (choose: ${PROVIDERS.join(", ")})`);
    return 1;
  }

  const defaults = defaultModels(provider);
  const asrModel = str(values["asr-model"]) || defaults.asr;
  const ttsModel = str(values["tts-model"]) || defaults.tts;
  const baseUrl = str(values["base-url"]);

  const keyVar = providerKeyVar(provider);
  let key = str(values.key);
  if (!key && keyVar) {
    out(`  ${style.grey(`${keyVar} — leave blank to set it later`)}`);
    key = ask(`${provider} API key`, "");
  }
  if (key && keyVar) upsertEnv(home, { [keyVar]: key });

  const enabled = !values["no-enable"];
  writeSpeechConfig(home, { provider, asrModel, ttsModel, baseUrl, enabled });

  out("");
  out(style.pass("✓ Voice configured"));
  out(`  ${style.grey("provider:")} ${provider}`);
  out(`  ${style.grey("asr:     ")} ${asrModel}`);
  out(`  ${style.grey("tts:     ")} ${ttsModel || style.warn("(none — provider has no TTS)")}`);
  if (provider === "local") {
    out(
      `  ${style.grey("server:  ")} ${baseUrl || "http://localhost:8000/v1"} ${style.grey("(run a local Qwen3 OpenAI-compatible server)")}`,
    );
  } else {
    out(
      `  ${style.grey("key:     ")} ${key ? "set" : style.warn(`not set — export ${keyVar} before use`)}`,
    );
  }
  out(`  ${style.grey("enabled: ")} ${enabled ? "yes" : "no"}`);
  out("");
  out(`  Next: ${style.teal("regent voice status")}`);
  return 0;
}

function setEnabled(profile: string, enabled: boolean): number {
  const home = regentHome(profile);
  const doc = readConfig(home);
  const speech = (
    typeof doc.speech === "object" && doc.speech !== null ? doc.speech : {}
  ) as Record<string, unknown>;
  speech.enabled = enabled;
  doc.speech = speech;
  writeConfig(home, doc);
  out(`voice ${enabled ? style.teal("enabled") : "disabled"}`);
  out(style.grey("(applies on the next `regent` command — the daemon reloads config each run)"));
  return 0;
}

interface VoiceStatus {
  enabled: boolean;
  asr: { provider: string; model: string; available: boolean };
  tts: { provider: string; model: string; available: boolean };
  vision: { input_mode: string };
  call: { fast_model: string };
}

async function voiceStatus(client: IRpcClient): Promise<number> {
  const res = await client.call<VoiceStatus>("voice.status", {}, 15_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  const s = res.value;
  const dot = (ok: boolean): string => (ok ? style.teal("●") : style.grey("○"));
  out(style.heading("Voice"));
  out(`  ${"enabled".padEnd(8)} ${s.enabled ? style.teal("yes") : style.grey("no")}`);
  out(`  ${"asr".padEnd(8)} ${dot(s.asr.available)} ${s.asr.provider}/${s.asr.model}`);
  out(`  ${"tts".padEnd(8)} ${dot(s.tts.available)} ${s.tts.provider}/${s.tts.model}`);
  out(`  ${"vision".padEnd(8)} ${s.vision.input_mode}`);
  if (s.call.fast_model) out(`  ${"fast".padEnd(8)} ${s.call.fast_model}`);
  if (!s.enabled) out(style.grey("\n  enable with: regent voice setup"));
  return 0;
}

interface VoiceModels {
  asr: { configured: { provider: string; model: string }; builtins: string[] };
  tts: { configured: { provider: string; model: string }; builtins: string[] };
}

async function voiceModels(client: IRpcClient): Promise<number> {
  const res = await client.call<VoiceModels>("voice.models", {}, 15_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  const v = res.value;
  out(style.heading("Voice providers"));
  out(
    `  ${"asr".padEnd(4)} ${style.value(`${v.asr.configured.provider}/${v.asr.configured.model}`)}`,
  );
  out(`       ${style.grey(`built-in: ${v.asr.builtins.join(", ")}`)}`);
  out(
    `  ${"tts".padEnd(4)} ${style.value(`${v.tts.configured.provider}/${v.tts.configured.model}`)}`,
  );
  out(`       ${style.grey(`built-in: ${v.tts.builtins.join(", ")}`)}`);
  return 0;
}

// --- config / env file helpers (mirror setupCommand) -----------------------

const str = (v: string | boolean | undefined): string => (typeof v === "string" ? v : "");

function ask(label: string, def: string): string {
  const answer = prompt(`  ${def ? `${label} [${def}]:` : `${label}:`}`);
  const value = (answer ?? "").trim();
  return value || def;
}

function readConfig(home: string): Record<string, unknown> {
  try {
    const parsed = YAML.parse(readFileSync(join(home, "config.yaml"), "utf8")) as unknown;
    if (parsed && typeof parsed === "object") return parsed as Record<string, unknown>;
  } catch {
    // no / invalid config.yaml — start fresh
  }
  return {};
}

function writeConfig(home: string, doc: Record<string, unknown>): void {
  if (doc._config_version === undefined) doc._config_version = 1;
  mkdirSync(home, { recursive: true });
  const tmp = join(home, `config.yaml.tmp.${process.pid}`);
  writeFileSync(tmp, YAML.stringify(doc));
  renameSync(tmp, join(home, "config.yaml"));
}

function writeSpeechConfig(
  home: string,
  opts: {
    provider: string;
    asrModel: string;
    ttsModel: string;
    baseUrl: string;
    enabled: boolean;
  },
): void {
  const doc = readConfig(home);
  applySpeechConfig(doc, opts);
  writeConfig(home, doc);
}

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
