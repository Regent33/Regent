// `regent setup` — first-time (and re-run) configuration: provider + model +
// API key. Secrets go to $REGENT_HOME/.env (owner-only, atomic write); provider/
// model are merged into config.yaml (preserving other keys, so re-running setup
// to switch provider actually takes effect).
//
// Two flows, one persistence path (domain/writeSetup.ts):
// - interactive TTY with no flags → the Ink wizard (arrow-key pickers fed by
//   the deacon's providers.catalog; falls back to the linear prompts if the
//   deacon can't be reached);
// - flags given or non-TTY → the linear prompt flow below (scriptable).
import { mkdirSync, readFileSync } from "node:fs";
import { parseFlags } from "@app/cli/args.ts";
import { out, printError } from "@app/cli/runtime.ts";
import { explainConfigFailure } from "@features/inspect/cli/deaconConfig.ts";
import { markSetupDone } from "@features/setup/domain/firstRun.ts";
import { selectSetupKey, writeConfig, writeEnv } from "@features/setup/domain/writeSetup.ts";
import { regentHome } from "@shared/infrastructure/deacon/locate.ts";
import { style } from "@shared/ui/style.ts";

// Mirrors ProviderKind::ALL in the deacon. `ollama` is the local daemon and
// `ollama-cloud` the hosted service: same wire protocol, different endpoint and
// a key, so they are two providers here exactly as they are two kinds there.
const PROVIDERS = [
  ...["anthropic", "openai", "openrouter", "groq", "deepseek", "together"],
  ...["mistral", "xai", "gemini", "moonshot", "zhipu", "dashscope"],
  ...["fireworks", "cerebras", "perplexity", "minimax", "nvidia"],
  // Open-weights hosts: same OpenAI-compatible wire, HF-style model ids.
  ...["sambanova", "hyperbolic", "novita", "deepinfra", "siliconflow"],
  ...["nebius", "chutes", "venice", "cohere", "github-models"],
  ...["ollama-cloud"],
  // Servers you run yourself — keyless by default, like the local daemon.
  ...["lmstudio", "llamacpp", "vllm", "litellm", "ollama"],
];

const LOCAL_PROVIDERS = new Set(["ollama", "lmstudio", "llamacpp", "vllm", "litellm"]);

const str = (v: string | boolean | undefined): string => (typeof v === "string" ? v : "");

export async function setupCommand(profile: string, args: string[]): Promise<number> {
  const { values } = parseFlags(args, {
    provider: { type: "string" },
    model: { type: "string" },
    "base-url": { type: "string" },
    key: { type: "string" },
    "key-stdin": { type: "boolean" },
  });
  if (values.key !== undefined) {
    printError("do not put secrets in command history; pipe the key to `regent setup --key-stdin`");
    return 2;
  }
  let pipedKey = "";
  if (values["key-stdin"] === true) {
    try {
      const streamed = readFileSync(0, "utf8");
      pipedKey = streamed.replace(/\r?\n$/, "");
    } catch (error) {
      printError(`could not read the setup key from stdin: ${String(error)}`);
      return 1;
    }
    if (!pipedKey || /[\r\n\0]/.test(pipedKey)) {
      printError("the setup key from stdin must be one non-empty line without NUL bytes");
      return 1;
    }
  }
  const anyFlag =
    values.provider !== undefined ||
    values.model !== undefined ||
    values["base-url"] !== undefined ||
    values["key-stdin"] === true;

  // Interactive default: the Ink wizard. Lazy import keeps the scripted path
  // free of Ink; if the deacon is unreachable the wizard defers back here.
  if (!anyFlag && process.stdin.isTTY) {
    const { runSetupWizard } = await import("./runSetupWizard.tsx");
    const code = await runSetupWizard(profile);
    if (code !== 2) return code; // 2 = wizard unavailable → linear fallback
    out(style.grey("falling back to prompt-based setup"));
  }

  return linearSetup(profile, {
    provider: str(values.provider),
    model: str(values.model),
    baseUrl: str(values["base-url"]),
    key: pipedKey,
  });
}

interface Prefills {
  provider: string;
  model: string;
  baseUrl: string;
  key: string;
}

// The scriptable line-prompt flow (also the non-TTY and no-deacon fallback).
async function linearSetup(profile: string, pre: Prefills): Promise<number> {
  const home = regentHome(profile);

  banner("Regent Setup");
  if (!process.stdin.isTTY) {
    out(`  ${style.grey("non-interactive input — defaults apply where no flag is given")}`);
  }
  section("Model & Provider", "Choose your AI provider, default model, and credentials.");

  let provider = pre.provider;
  if (!provider) {
    out(`  ${style.grey(`providers: ${PROVIDERS.join(", ")}`)}`);
    provider = ask("Provider", "anthropic");
  }
  if (!PROVIDERS.includes(provider)) {
    printError(`unknown provider "${provider}" (choose: ${PROVIDERS.join(", ")})`);
    return 1;
  }

  // Every self-hosted backend is key-optional. Only Ollama exposes a stable
  // discovery endpoint worth probing during setup; the others still accept
  // --base-url and an optional --key for protected local servers.
  const isLocal = LOCAL_PROVIDERS.has(provider);
  if (provider === "ollama") await showOllamaStatus();

  const defaultModel =
    { ollama: "llama3.2", "ollama-cloud": "glm-5.2" }[provider] ??
    (isLocal ? "" : "claude-sonnet-4-6");
  let model = pre.model;
  if (!model) model = ask("Default model", defaultModel);
  if (!model) {
    printError(`a model id is required for ${provider} (use --model <id>)`);
    return 1;
  }

  // Base URL is flag-only in the wizard; here it stays prompt-reachable for
  // scripts and air-gapped OpenAI-compatible hosts.
  let baseUrl = pre.baseUrl;
  if (!baseUrl && !isLocal) {
    out(`  ${style.grey("custom API endpoint — leave blank for the provider default")}`);
    baseUrl = ask("Base URL", "");
  }

  let key = selectSetupKey(pre.key, isLocal, process.env.REGENT_API_KEY);
  if (!key && !isLocal) {
    out(
      `  ${style.grey("API key is visible — leave blank to set REGENT_API_KEY in the env later")}`,
    );
    key = ask("API key", "");
  }

  out("");
  section(
    "Constitution",
    "Values layer: character grounded in Christian biblical values, with hard boundaries.",
  );
  // Always on — the constitution is a core, non-disableable layer.
  out(`  ${style.grey("always enabled — view or edit it later with `regent persona`")}`);

  mkdirSync(home, { recursive: true });
  if (!key && !isLocal) {
    out(style.warn("warning: no API key set — export REGENT_API_KEY before running the agent"));
  }
  writeEnv(home, key);
  const saved = writeConfig(home, provider, model, baseUrl, true, key !== "");
  if (saved.status !== "ok") {
    printError(explainConfigFailure(saved));
    return 1;
  }
  markSetupDone(home);

  summary(home, provider, model, baseUrl, key, isLocal);
  return 0;
}

// Ollama status for the local path: running? which models are pulled? Keeps
// setup honest offline — a dead daemon or empty library is said out loud.
async function showOllamaStatus(): Promise<void> {
  try {
    const r = await fetch("http://localhost:11434/api/tags", {
      signal: AbortSignal.timeout(1500),
    });
    const tags = (await r.json()) as { models?: Array<{ name: string }> };
    const names = (tags.models ?? []).map((m) => m.name);
    out(
      names.length
        ? `  ${style.grey(`ollama is running — installed models: ${names.join(", ")}`)}`
        : `  ${style.grey("ollama is running but has no models — run `ollama pull llama3.2` first")}`,
    );
  } catch {
    out(
      `  ${style.grey("ollama is not reachable at localhost:11434 — install it from https://ollama.com, then `ollama pull <model>`")}`,
    );
  }
}

const BOX_WIDTH = 52;

// A boxed teal banner (setup-header style).
function banner(title: string): void {
  const inner = BOX_WIDTH - 2;
  const label = `♚  ${title}`;
  const pad = " ".repeat(Math.max(0, inner - 1 - label.length));
  out("");
  out(style.teal(`╭${"─".repeat(inner)}╮`));
  out(`${style.teal("│")} ${style.bold(label)}${pad}${style.teal("│")}`);
  out(style.teal(`╰${"─".repeat(inner)}╯`));
  out("");
}

// A named section header with a rule and a one-line description.
function section(title: string, description: string): void {
  out(style.heading(title));
  out(style.teal("━".repeat(BOX_WIDTH)));
  out(`${style.grey(description)}\n`);
}

// The completion summary: what was written + next steps.
function summary(
  home: string,
  provider: string,
  model: string,
  baseUrl: string,
  key: string,
  isLocal: boolean,
): void {
  out("");
  out(style.pass("✓ Setup complete"));
  out(`  ${style.grey("home:    ")} ${home}`);
  out(`  ${style.grey("provider:")} ${provider}`);
  out(`  ${style.grey("model:   ")} ${model}`);
  if (baseUrl) out(`  ${style.grey("base url:")} ${baseUrl}`);
  out(`  ${style.grey("constitution:")} enabled`);
  const keyState = key
    ? "set"
    : isLocal
      ? style.grey("optional — only needed if your local server requires auth")
      : style.warn("not set — export REGENT_API_KEY before running the agent");
  out(`  ${style.grey("api key: ")} ${keyState}`);
  out("");
  out(`  Next: ${style.teal("regent doctor")}  →  ${style.teal("regent chat")}`);
  out("");
}

// Synchronous line prompt via Bun's built-in `prompt`. Reliable for sequential
// questions, where node:readline's async `question` proved flaky under Bun.
function ask(label: string, def: string): string {
  const answer = prompt(`  ${def ? `${label} [${def}]:` : `${label}:`}`);
  const value = (answer ?? "").trim();
  return value || def;
}
