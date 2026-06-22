// Speech provider catalog + the pure config helpers. No I/O, no UI — the wizard
// (voiceCommand) and the tests both build on this.

export interface ProviderInfo {
  readonly id: string;
  readonly label: string;
  readonly blurb: string;
  readonly base: string; // OpenAI-compatible base URL
  readonly keyVar: string | null; // env var holding the API key (null = none)
  readonly keyUrl: string; // where to get the key
}

// Ordered easiest-first; the setup menu defaults to #1.
export const PROVIDERS: readonly ProviderInfo[] = [
  {
    id: "groq",
    label: "Groq",
    blurb: "free Whisper speech-to-text — just a free API key (easiest)",
    base: "https://api.groq.com/openai/v1",
    keyVar: "GROQ_API_KEY",
    keyUrl: "https://console.groq.com/keys",
  },
  {
    id: "openai",
    label: "OpenAI",
    blurb: "Whisper STT + spoken replies (TTS) — one API key",
    base: "https://api.openai.com/v1",
    keyVar: "OPENAI_API_KEY",
    keyUrl: "https://platform.openai.com/api-keys",
  },
  {
    id: "qwen",
    label: "Qwen (DashScope)",
    blurb: "Alibaba Qwen3 STT + TTS — one API key",
    base: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
    keyVar: "DASHSCOPE_API_KEY",
    keyUrl: "https://dashscope.console.aliyun.com/apiKey",
  },
  {
    id: "local",
    label: "Local server",
    blurb: "your own Qwen3 OpenAI-compatible server, e.g. vLLM (advanced)",
    base: "http://localhost:8000/v1",
    keyVar: null,
    keyUrl: "",
  },
];

/** Resolve a provider by id (accepts the `dashscope` alias for `qwen`). */
export function findProvider(id: string): ProviderInfo | undefined {
  const norm = id.toLowerCase() === "dashscope" ? "qwen" : id.toLowerCase();
  return PROVIDERS.find((p) => p.id === norm);
}

/** Env var holding a provider's API key. */
export function providerKeyVar(provider: string): string | null {
  if (provider === "dashscope") return "DASHSCOPE_API_KEY";
  return PROVIDERS.find((p) => p.id === provider)?.keyVar ?? null;
}

/** Sensible default ASR/TTS model ids per provider; Qwen is the headline. */
export function defaultModels(provider: string): { asr: string; tts: string } {
  switch (provider) {
    case "groq":
      return { asr: "whisper-large-v3-turbo", tts: "" };
    case "openai":
      return { asr: "whisper-1", tts: "gpt-4o-mini-tts" };
    default:
      return { asr: "qwen3-asr-1.7b", tts: "qwen3-tts-1.7b" };
  }
}

/** Merge speech settings into a parsed config.yaml doc, preserving other keys. */
export function applySpeechConfig(
  doc: Record<string, unknown>,
  opts: { provider: string; asrModel: string; ttsModel: string; baseUrl: string; enabled: boolean },
): void {
  const speech = (
    typeof doc.speech === "object" && doc.speech !== null ? doc.speech : {}
  ) as Record<string, unknown>;
  speech.enabled = opts.enabled;
  speech.asr = { provider: opts.provider, model: opts.asrModel, base_url: opts.baseUrl };
  speech.tts = { provider: opts.provider, model: opts.ttsModel, base_url: opts.baseUrl };
  doc.speech = speech;
}
