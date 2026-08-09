// Provider + API-key diagnostics shared by `regent doctor` and in-chat
// `/doctor`. Reads the same current multi-provider config shape as the deacon,
// while preserving the legacy `model:` fallback for older installations.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import YAML from "yaml";

export const DEFAULT_BASE: Record<string, string> = {
  anthropic: "https://api.anthropic.com",
  openai: "https://openrouter.ai/api",
  openrouter: "https://openrouter.ai/api",
  groq: "https://api.groq.com/openai",
  deepseek: "https://api.deepseek.com",
  together: "https://api.together.xyz",
  ollama: "http://localhost:11434",
  "ollama-cloud": "https://ollama.com",
  mistral: "https://api.mistral.ai",
  xai: "https://api.x.ai",
  gemini: "https://generativelanguage.googleapis.com/v1beta/openai",
  moonshot: "https://api.moonshot.ai",
  zhipu: "https://open.bigmodel.cn/api/paas/v4",
  dashscope: "https://dashscope-intl.aliyuncs.com/compatible-mode",
  fireworks: "https://api.fireworks.ai/inference",
  cerebras: "https://api.cerebras.ai",
  perplexity: "https://api.perplexity.ai",
  minimax: "https://api.minimax.io",
  nvidia: "https://integrate.api.nvidia.com",
  sambanova: "https://api.sambanova.ai",
  hyperbolic: "https://api.hyperbolic.xyz",
  novita: "https://api.novita.ai/v3/openai",
  deepinfra: "https://api.deepinfra.com/v1/openai",
  siliconflow: "https://api.siliconflow.cn",
  nebius: "https://api.studio.nebius.com",
  chutes: "https://llm.chutes.ai",
  venice: "https://api.venice.ai/api",
  cohere: "https://api.cohere.ai/compatibility",
  "github-models": "https://models.github.ai/inference",
  lmstudio: "http://localhost:1234",
  llamacpp: "http://localhost:8080",
  vllm: "http://localhost:8000",
  litellm: "http://localhost:4000",
};

const LOCAL = new Set(["ollama", "lmstudio", "llamacpp", "vllm", "litellm"]);

export const maskKey = (key: string): string =>
  key.length <= 12 ? "set" : `${key.slice(0, 8)}…${key.slice(-4)}`;

export function readDotEnvValue(home: string, name: string): string | undefined {
  try {
    for (const raw of readFileSync(join(home, ".env"), "utf8").split("\n")) {
      const trimmed = raw.trim();
      if (trimmed.startsWith(`${name}=`)) {
        const value = trimmed
          .slice(name.length + 1)
          .replace(/^"|"$/g, "")
          .trim();
        return value || undefined;
      }
    }
  } catch {
    // no .env
  }
  return undefined;
}

export const readDotEnvKey = (home: string): string | undefined =>
  readDotEnvValue(home, "REGENT_API_KEY");

interface ProviderSpec {
  readonly kind?: string;
  readonly base_url?: string;
  readonly api_key_env?: string;
}

interface PrimaryObject {
  readonly provider?: string;
  readonly model?: string;
  readonly key_slot?: number;
}

export interface ProviderInfo {
  readonly provider: string;
  readonly kind: string;
  readonly model: string;
  readonly endpoint: string;
  readonly keyCandidates: readonly string[];
  readonly needsKey: boolean;
  readonly isLocal: boolean;
}

interface ConfigDoc {
  readonly model?: { provider?: string; default?: string; base_url?: string };
  readonly agents_defaults?: { primary?: unknown };
  readonly providers?: Record<string, ProviderSpec>;
}

function conventionalKey(kind: string): string {
  if (kind === "github-models") return "GITHUB_TOKEN";
  if (kind === "ollama-cloud") return "OLLAMA_API_KEY";
  return `${kind.replaceAll("-", "_").toUpperCase()}_API_KEY`;
}

function primaryObject(value: unknown): PrimaryObject | undefined {
  if (typeof value === "object" && value !== null) return value as PrimaryObject;
  if (typeof value !== "string") return undefined;
  const [provider, ...model] = value.split("/");
  return provider && model.length > 0 ? { provider, model: model.join("/") } : undefined;
}

export function readProviderInfo(home: string): ProviderInfo {
  let doc: ConfigDoc | null = null;
  try {
    doc = YAML.parse(readFileSync(join(home, "config.yaml"), "utf8")) as ConfigDoc;
  } catch {
    // no or malformed config: doctor will separately validate config.yaml
  }

  const primary = primaryObject(doc?.agents_defaults?.primary);
  if (primary?.provider && primary.model) {
    const spec = doc?.providers?.[primary.provider];
    const kind = spec?.kind ?? primary.provider;
    const rawKeyEnv = spec ? (spec.api_key_env?.trim() ?? "") : conventionalKey(kind);
    const keyEnv =
      rawKeyEnv && primary.key_slot && primary.key_slot >= 2
        ? `${rawKeyEnv}_${primary.key_slot}`
        : rawKeyEnv;
    return {
      provider: primary.provider,
      kind,
      model: primary.model,
      endpoint: spec?.base_url || DEFAULT_BASE[kind] || "?",
      keyCandidates: keyEnv ? [keyEnv] : [],
      needsKey: keyEnv !== "",
      isLocal: LOCAL.has(kind),
    };
  }

  const provider = doc?.model?.provider ?? "anthropic";
  const kindKey = conventionalKey(provider);
  const providerSlots = [kindKey, ...Array.from({ length: 7 }, (_, i) => `${kindKey}_${i + 2}`)];
  return {
    provider,
    kind: provider,
    model: doc?.model?.default ?? "—",
    endpoint: doc?.model?.base_url || DEFAULT_BASE[provider] || "?",
    keyCandidates: [...providerSlots, "REGENT_API_KEY"],
    needsKey: !LOCAL.has(provider),
    isLocal: LOCAL.has(provider),
  };
}

export interface LocalEndpointProbe {
  readonly ok: boolean;
  readonly detail: string;
}

interface LocalEndpointProbeOptions {
  readonly fetch?: typeof fetch;
  readonly timeoutMs?: number;
  readonly apiKey?: string;
}

function localProbeUrl(info: ProviderInfo): string | undefined {
  if (!info.isLocal) return undefined;
  try {
    const url = new URL(info.endpoint);
    const basePath = url.pathname.replace(/\/+$/, "");
    const suffix =
      info.kind === "ollama" ? "/api/tags" : basePath.endsWith("/v1") ? "/models" : "/v1/models";
    url.pathname = `${basePath}${suffix}`;
    url.search = "";
    url.hash = "";
    return url.toString();
  } catch {
    return undefined;
  }
}

/**
 * Fast, read-only reachability check for a configured self-hosted provider.
 * It deliberately uses model discovery instead of a completion, so Doctor
 * neither spends tokens nor requires an API key for a normal local install.
 */
export async function probeLocalProviderEndpoint(
  info: ProviderInfo,
  options: LocalEndpointProbeOptions = {},
): Promise<LocalEndpointProbe> {
  const url = localProbeUrl(info);
  if (!url) {
    return {
      ok: false,
      detail: `invalid local endpoint '${info.endpoint}' - correct providers.${info.provider}.base_url`,
    };
  }

  const timeoutMs = options.timeoutMs ?? 2_000;
  const headers = options.apiKey ? { Authorization: `Bearer ${options.apiKey}` } : undefined;
  try {
    const response = await (options.fetch ?? fetch)(url, {
      method: "GET",
      ...(headers ? { headers } : {}),
      signal: AbortSignal.timeout(timeoutMs),
    });
    if (response.ok) {
      return { ok: true, detail: `reachable at ${url} (HTTP ${response.status})` };
    }
    const status = response.statusText
      ? `HTTP ${response.status} ${response.statusText}`
      : `HTTP ${response.status}`;
    return {
      ok: false,
      detail: `reached ${url}, but model discovery returned ${status}`,
    };
  } catch (error) {
    const message = error instanceof Error && error.message ? error.message : String(error);
    const timedOut = error instanceof Error && error.name === "TimeoutError";
    return {
      ok: false,
      detail: timedOut
        ? `connection to ${url} timed out after ${timeoutMs}ms - start ${info.kind} or correct providers.${info.provider}.base_url`
        : `connection failed to ${url} - start ${info.kind} or correct providers.${info.provider}.base_url (${message})`,
    };
  }
}

export interface ActiveKeyInfo {
  readonly value?: string;
  readonly source?: string;
  readonly shadowed?: string;
}

export function activeProviderKey(home: string, info: ProviderInfo): ActiveKeyInfo {
  for (const name of info.keyCandidates) {
    const envKey = process.env[name]?.trim() || undefined;
    const dotenvKey = readDotEnvValue(home, name);
    if (envKey) {
      return {
        value: envKey,
        source: `shell env ${name}`,
        ...(dotenvKey && dotenvKey !== envKey ? { shadowed: name } : {}),
      };
    }
    if (dotenvKey) return { value: dotenvKey, source: `.env ${name}` };
  }
  return {};
}

/** Plain-text diagnostics for the in-chat `/doctor` note. */
export function providerKeyDiagnostics(home: string): string {
  const info = readProviderInfo(home);
  const active = activeProviderKey(home, info);
  const lines = [
    "Diagnostics",
    `  provider   ${info.provider} · ${info.model}`,
    `  endpoint   ${info.endpoint}`,
  ];
  if (!active.value && info.needsKey) {
    lines.push(`  API key    ✗ none — set ${info.keyCandidates[0] ?? "the provider key"}`);
  } else if (!active.value) {
    lines.push("  API key    not required for this local provider");
  } else {
    lines.push(`  API key    ${maskKey(active.value)} (from ${active.source})`);
    if (active.shadowed) {
      lines.push(
        `  ⚠ shell ${active.shadowed} overrides a different value in .env.`,
        `    Unset it (PowerShell: Remove-Item Env:${active.shadowed}) to use the saved key.`,
      );
    }
  }
  lines.push("", "A 401 means the selected provider rejected its configured key or model.");
  return lines.join("\n");
}
