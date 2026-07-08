// `regent setup` — first-time (and re-run) configuration: provider + model +
// API key. Secrets go to $REGENT_HOME/.env (owner-only, atomic write); provider/
// model are merged into config.yaml (preserving other keys, so re-running setup
// to switch provider actually takes effect).
import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { parseFlags } from "@app/cli/args.ts";
import { out, printError } from "@app/cli/runtime.ts";
import { regentHome } from "@shared/infrastructure/deacon/locate.ts";
import { lockDownFile } from "@shared/infrastructure/storage/lockdown.ts";
import { style } from "@shared/ui/style.ts";
import YAML from "yaml";

const PROVIDERS = ["anthropic", "openai", "openrouter", "groq", "deepseek", "together", "ollama"];

const str = (v: string | boolean | undefined): string => (typeof v === "string" ? v : "");

// A sectioned interactive wizard: a boxed
// banner, named sections with rules, prompts that show their default + a short
// description, and a completion summary with next steps. Regent's config
// surface is small, so there is one section (Model & Provider) —
// gateway/tools/cron live behind their own commands.
export async function setupCommand(profile: string, args: string[]): Promise<number> {
  const { values } = parseFlags(args, {
    provider: { type: "string" },
    model: { type: "string" },
    "base-url": { type: "string" },
    key: { type: "string" },
    constitution: { type: "string" },
  });
  const home = regentHome(profile);

  banner("Regent Setup");
  section("Model & Provider", "Choose your AI provider, default model, and credentials.");

  let provider = str(values.provider);
  if (!provider) {
    out(`  ${style.grey(`providers: ${PROVIDERS.join(", ")}`)}`);
    provider = ask("Provider", "anthropic");
  }
  if (!PROVIDERS.includes(provider)) {
    printError(`unknown provider "${provider}" (choose: ${PROVIDERS.join(", ")})`);
    return 1;
  }

  // Local/offline provider: no API key, and show what's installed so the
  // model prompt isn't a guess. Models download with `ollama pull <name>`.
  const isLocal = provider === "ollama";
  if (isLocal) await showOllamaStatus();

  let model = str(values.model);
  if (!model) model = ask("Default model", isLocal ? "llama3.2" : "claude-sonnet-4-6");

  let baseUrl = str(values["base-url"]);
  if (!baseUrl && !isLocal) {
    out(`  ${style.grey("custom API endpoint — leave blank for the provider default")}`);
    baseUrl = ask("Base URL", "");
  }

  let key = str(values.key) || process.env.REGENT_API_KEY || "";
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
  const constitution = true;

  mkdirSync(home, { recursive: true });
  writeEnv(home, key);
  writeConfig(home, provider, model, baseUrl, constitution);

  summary(home, provider, model, baseUrl, key, constitution);
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
  constitution: boolean,
): void {
  out("");
  out(style.pass("✓ Setup complete"));
  out(`  ${style.grey("home:    ")} ${home}`);
  out(`  ${style.grey("provider:")} ${provider}`);
  out(`  ${style.grey("model:   ")} ${model}`);
  if (baseUrl) out(`  ${style.grey("base url:")} ${baseUrl}`);
  if (constitution) out(`  ${style.grey("constitution:")} enabled`);
  out(
    `  ${style.grey("api key: ")} ${key ? "set" : style.warn("not set — export REGENT_API_KEY before running the agent")}`,
  );
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

// Upsert REGENT_API_KEY in .env, preserving other lines. Atomic temp→rename at
// 0600; on Windows an owner-only ACL is applied after the rename.
function writeEnv(home: string, key: string): void {
  if (!key) {
    out(style.warn("warning: no API key set — export REGENT_API_KEY before running the agent"));
    return;
  }
  const path = join(home, ".env");
  const kept: string[] = [];
  try {
    for (const line of readFileSync(path, "utf8").split("\n")) {
      const t = line.trim();
      if (t === "" || t.startsWith("REGENT_API_KEY=")) continue;
      kept.push(line);
    }
  } catch {
    // no existing .env — fine
  }
  kept.push(`REGENT_API_KEY=${key}`);
  const tmp = join(home, `.env.tmp.${process.pid}`);
  writeFileSync(tmp, `${kept.join("\n")}\n`, { mode: 0o600 });
  renameSync(tmp, path);
  lockDownFile(path);
}

// Merge the chosen provider/model/base_url into config.yaml, preserving every
// other key. Crucially this UPDATES an existing config (re-running `setup` to
// switch provider must take effect) instead of skipping it. When no base_url is
// given the key is removed so the deacon uses the provider's own default
// endpoint (e.g. openrouter → openrouter.ai) rather than a stale override.
function writeConfig(
  home: string,
  provider: string,
  model: string,
  baseUrl: string,
  constitution: boolean,
): void {
  const path = join(home, "config.yaml");
  let doc: Record<string, unknown> = {};
  try {
    const parsed = YAML.parse(readFileSync(path, "utf8")) as unknown;
    if (parsed && typeof parsed === "object") doc = parsed as Record<string, unknown>;
  } catch {
    // no / invalid config.yaml — start fresh
  }
  doc._config_version = doc._config_version ?? 1;
  const m = (typeof doc.model === "object" && doc.model !== null ? doc.model : {}) as Record<
    string,
    unknown
  >;
  m.provider = provider;
  m.default = model;
  // undefined keys are omitted by YAML.stringify — clears a stale override.
  m.base_url = baseUrl ? baseUrl : undefined;
  doc.model = m;
  // The deacon seeds/clears the constitution persona row from this flag on boot.
  doc.constitution = { enabled: constitution };

  mkdirSync(home, { recursive: true });
  const tmp = join(home, `config.yaml.tmp.${process.pid}`);
  writeFileSync(tmp, YAML.stringify(doc), { mode: 0o644 });
  renameSync(tmp, path);
}
