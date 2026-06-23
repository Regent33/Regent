// `regent voice` — set up and inspect the voice (ASR/TTS) stack. Off by default.
// `setup` is the one intuitive command: pick a provider, save the key, and it
// configures BOTH the daemon (config.yaml) and the gateway (.env) at once — so
// voice works in chat and over Telegram from a single command. `test` verifies
// it end to end; status/models read the daemon (see voiceInspect).
import { parseFlags } from "@app/cli/args.ts";
import { out, printError, withClient } from "@app/cli/runtime.ts";
import { regentHome } from "@shared/infrastructure/daemon/locate.ts";
import { withSpinner } from "@shared/ui/consoleSpinner.ts";
import { style } from "@shared/ui/style.ts";
import { readConfig, upsertEnv, writeConfig } from "./voiceFiles.ts";
import { voiceModels, voiceStatus, voiceTest } from "./voiceInspect.ts";
import {
  PROVIDERS,
  type ProviderInfo,
  applySpeechConfig,
  defaultModels,
  findProvider,
} from "./voiceProviders.ts";
import { voiceServe } from "./voiceServe.ts";

export async function voiceCommand(profile: string, args: string[]): Promise<number> {
  switch (args[0]) {
    case "setup":
      return voiceSetup(profile, args.slice(1));
    case "enable":
      return setEnabled(profile, true);
    case "disable":
      return setEnabled(profile, false);
    case "serve":
      return voiceServe();
    case "test":
      return withClient(profile, voiceTest);
    case "status":
      return withClient(profile, voiceStatus);
    case "models":
      return withClient(profile, voiceModels);
    default:
      printError("usage: regent voice setup | serve | test | status | models | enable | disable");
      out(style.grey("  start here: regent voice setup   ·   local server: regent voice serve"));
      return 1;
  }
}

async function voiceSetup(profile: string, args: string[]): Promise<number> {
  const { values } = parseFlags(args, {
    provider: { type: "string" },
    "asr-model": { type: "string" },
    "tts-model": { type: "string" },
    "base-url": { type: "string" },
    key: { type: "string" },
    "no-enable": { type: "boolean" },
  });
  const home = regentHome(profile);

  // The interactive menu needs a real terminal. When piped (e.g. run from inside
  // chat as a subprocess), it can't read input — so require flags instead of
  // printing a menu nobody can answer (that stranded `/voice setup` in chat).
  if (!str(values.provider) && !process.stdin.isTTY) {
    printError("`regent voice setup` needs a terminal for the interactive menu.");
    out(style.grey("  Run it in your shell, or pass flags, e.g.:"));
    out(style.grey("    regent voice setup --provider groq --key <key>"));
    return 1;
  }

  banner("Regent Voice");
  out(`  ${style.grey("Send a voice note, get a spoken reply. Pick where speech runs:")}\n`);

  const p = resolveProvider(str(values.provider));
  if (!p) {
    printError(`unknown provider — choose: ${PROVIDERS.map((x) => x.id).join(", ")}`);
    return 1;
  }

  const defaults = defaultModels(p.id);
  const asrModel = str(values["asr-model"]) || defaults.asr;
  const ttsModel = str(values["tts-model"]) || defaults.tts;
  const base = str(values["base-url"]) || p.base;

  let key = str(values.key);
  if (p.keyVar && !key) {
    out(`\n  ${style.grey(`Get a free/paid key: ${p.keyUrl}`)}`);
    key = ask(`${p.label} API key`, "");
  }

  // One setup configures both planes: config.yaml (daemon/chat) + .env (gateway).
  const enabled = !values["no-enable"];
  const doc = readConfig(home);
  applySpeechConfig(doc, {
    provider: p.id,
    asrModel,
    ttsModel,
    baseUrl: p.id === "local" ? base : "",
    enabled,
  });
  writeConfig(home, doc);
  const env: Record<string, string> = {
    REGENT_SPEECH_BASE_URL: base,
    REGENT_SPEECH_ASR_MODEL: asrModel,
  };
  if (ttsModel) env.REGENT_SPEECH_TTS_MODEL = ttsModel;
  if (key) {
    env.REGENT_SPEECH_API_KEY = key;
    if (p.keyVar) env[p.keyVar] = key;
  }
  upsertEnv(home, env);

  summary(p, asrModel, ttsModel, base, key);
  out("");
  if (enabled) await ensureModels(profile);
  out(
    `  ${style.bold("Next:")} ${style.teal("regent voice test")} ${style.grey("— verify it works")}`,
  );
  out(`  ${style.grey("Then send a voice note in chat, or run the gateway for Telegram.")}`);
  return 0;
}

/** Resolve the provider from a flag, or prompt with a numbered menu. */
function resolveProvider(flag: string): ProviderInfo | undefined {
  if (flag) return findProvider(flag);
  out(style.heading("  Speech provider"));
  PROVIDERS.forEach((p, i) =>
    out(
      `    ${style.teal(String(i + 1))}. ${style.bold(p.label.padEnd(15))} ${style.grey(p.blurb)}`,
    ),
  );
  const ans = ask("  Choose 1-4 or a name", "1");
  const n = Number(ans);
  if (Number.isInteger(n) && n >= 1 && n <= PROVIDERS.length) return PROVIDERS[n - 1];
  return findProvider(ans);
}

function summary(p: ProviderInfo, asr: string, tts: string, base: string, key: string): void {
  out("");
  out(`${style.pass("✓ Voice configured")} ${style.grey(`(${p.label})`)}`);
  out(`  ${style.grey("speech-to-text:")} ${asr}`);
  out(
    `  ${style.grey("text-to-speech:")} ${tts || style.warn("none — this provider does STT only (voice in, text out)")}`,
  );
  if (p.id === "local") {
    out(`  ${style.grey("server:        ")} ${base}`);
    out(`  ${style.teal("→ start it with: regent voice serve")} ${style.grey("(one command)")}`);
  } else {
    out(
      `  ${style.grey("api key:       ")} ${key ? "saved" : style.warn(`not set — re-run setup or add ${p.keyVar} to .env`)}`,
    );
  }
}

async function setEnabled(profile: string, enabled: boolean): Promise<number> {
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
  if (enabled) await ensureModels(profile); // download-on-enable
  return 0;
}

// Ask the daemon to download configured local weights (idempotent). Empty
// weights ⇒ nothing to fetch (hosted provider / a server you run).
async function ensureModels(profile: string): Promise<void> {
  await withClient(profile, async (client) => {
    const res = await withSpinner("downloading models…", () =>
      client.call<{ downloaded: string[]; note?: string }>("voice.ensure_models", {}, 600_000),
    );
    if (!res.ok) {
      printError(`model download failed: ${res.error.message}`);
      return 1;
    }
    if (res.value.downloaded.length) {
      out(`  ${style.grey("downloaded:")} ${res.value.downloaded.join(", ")}`);
    } else {
      out(`  ${style.grey("no model download needed (uses the provider's API/server)")}`);
    }
    return 0;
  });
}

// --- small UI helpers ------------------------------------------------------

const str = (v: string | boolean | undefined): string => (typeof v === "string" ? v : "");

function ask(label: string, def: string): string {
  const answer = prompt(`  ${def ? `${label} [${def}]:` : `${label}:`}`);
  const value = (answer ?? "").trim();
  return value || def;
}

function banner(title: string): void {
  out("");
  out(`  ${style.teal("♚")}  ${style.bold(title)}`);
  out(`  ${style.teal("━".repeat(title.length + 4))}`);
}
