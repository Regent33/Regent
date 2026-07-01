// Provider + API-key diagnostics shared by `regent doctor` (shell) and the
// in-chat `/doctor` command. Pure: reads config.yaml + .env + process.env, no
// deacon. The #1 cause of HTTP 401 is a shell-exported REGENT_API_KEY shadowing
// the .env key (real env wins), so this surfaces the key's source explicitly.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import YAML from "yaml";

// Default endpoint per provider (mirrors the deacon's provider_factory).
export const DEFAULT_BASE: Record<string, string> = {
  anthropic: "https://api.anthropic.com",
  openai: "https://openrouter.ai/api",
  openrouter: "https://openrouter.ai/api",
  groq: "https://api.groq.com/openai",
  deepseek: "https://api.deepseek.com",
  together: "https://api.together.xyz",
  ollama: "http://localhost:11434",
};

export const maskKey = (k: string): string =>
  k.length <= 12 ? "set" : `${k.slice(0, 8)}…${k.slice(-4)}`;

export function readDotEnvKey(home: string): string | undefined {
  try {
    for (const raw of readFileSync(join(home, ".env"), "utf8").split("\n")) {
      const t = raw.trim();
      if (t.startsWith("REGENT_API_KEY=")) {
        return t.slice("REGENT_API_KEY=".length).replace(/^"|"$/g, "").trim();
      }
    }
  } catch {
    // no .env
  }
  return undefined;
}

export interface ProviderInfo {
  readonly provider: string;
  readonly model: string;
  readonly endpoint: string;
}

export function readProviderInfo(home: string): ProviderInfo {
  let provider = "anthropic";
  let model = "—";
  let baseUrl: string | undefined;
  try {
    const doc = YAML.parse(readFileSync(join(home, "config.yaml"), "utf8")) as {
      model?: { provider?: string; default?: string; base_url?: string };
    } | null;
    provider = doc?.model?.provider ?? "anthropic";
    model = doc?.model?.default ?? "—";
    baseUrl = doc?.model?.base_url;
  } catch {
    // no config.yaml
  }
  return { provider, model, endpoint: baseUrl ?? DEFAULT_BASE[provider] ?? "?" };
}

/** Plain-text diagnostics for the in-chat `/doctor` note. */
export function providerKeyDiagnostics(home: string): string {
  const { provider, model, endpoint } = readProviderInfo(home);
  const envKey = process.env.REGENT_API_KEY?.trim();
  const dotenvKey = readDotEnvKey(home);
  const active = envKey || dotenvKey;
  const lines = ["Diagnostics", `  provider   ${provider} · ${model}`, `  endpoint   ${endpoint}`];
  if (!active) {
    lines.push("  API key    ✗ none — run `regent setup` in a terminal");
  } else {
    lines.push(`  API key    ${maskKey(active)} (from ${envKey ? "shell env" : ".env"})`);
    if (envKey && dotenvKey && envKey !== dotenvKey) {
      lines.push(
        "  ⚠ a shell-exported REGENT_API_KEY is OVERRIDING your .env key.",
        "    Unset it (PowerShell: Remove-Item Env:REGENT_API_KEY) to use the setup key.",
      );
    }
  }
  lines.push("", "A 401 means the provider rejected this key — verify the key + model are valid.");
  return lines.join("\n");
}
