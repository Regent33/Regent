// Builds the child environment the `regent-gateway` process runs with: merges
// $REGENT_HOME/.env, then resolves the SAME primary->fallback provider chain the
// deacon uses out of config.yaml so the gateway routes identically.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import YAML from "yaml";

// Merge $REGENT_HOME/.env into the child env (so REGENT_TELEGRAM_TOKEN etc.
// reach the gateway); the real environment always wins.
export function gatewayEnv(home: string): NodeJS.ProcessEnv {
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
      if (model?.provider && env.REGENT_PROVIDER === undefined)
        env.REGENT_PROVIDER = model.provider;
      if (model?.base_url && env.REGENT_BASE_URL === undefined)
        env.REGENT_BASE_URL = model.base_url;
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
function resolveProviderChain(cfg: GatewayConfig | null, env: NodeJS.ProcessEnv): ChainLink[] {
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
