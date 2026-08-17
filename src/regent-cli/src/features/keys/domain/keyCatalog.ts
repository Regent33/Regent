export type KeyGroup =
  | "AI models"
  | "Local models"
  | "Search"
  | "Messaging"
  | "Speech"
  | "Vision"
  | "Image generation"
  | "Video generation"
  | "Integrations";

export interface ManagedKey {
  readonly env: string;
  readonly label: string;
  readonly group: KeyGroup;
}

export const MAX_KEY_SLOTS = 8;

const row = (env: string, label: string, group: KeyGroup): ManagedKey => ({ env, label, group });

/** Offline CLI mirror of the credentials managed without a running deacon. */
export const MANAGED_KEYS: readonly ManagedKey[] = [
  row("ANTHROPIC_API_KEY", "Anthropic key", "AI models"),
  row("OPENAI_API_KEY", "OpenAI key", "AI models"),
  row("OPENROUTER_API_KEY", "OpenRouter key", "AI models"),
  row("GROQ_API_KEY", "Groq key", "AI models"),
  row("DEEPSEEK_API_KEY", "DeepSeek key", "AI models"),
  row("TOGETHER_API_KEY", "Together key", "AI models"),
  // "Local models" to match what the deacon's classifier reports and the app
  // renders — OLLAMA is in its LOCAL patterns. Listing it under AI models here
  // put the same key in two different sections depending on which surface you
  // opened.
  row("OLLAMA_API_KEY", "Ollama key (Ollama Cloud)", "Local models"),
  row("MISTRAL_API_KEY", "Mistral key", "AI models"),
  row("XAI_API_KEY", "xAI (Grok) key", "AI models"),
  row("GEMINI_API_KEY", "Google Gemini key", "AI models"),
  row("MOONSHOT_API_KEY", "Moonshot (Kimi) key", "AI models"),
  row("ZHIPU_API_KEY", "Zhipu (GLM/Z.AI) key", "AI models"),
  row("DASHSCOPE_API_KEY", "DashScope (Qwen) key", "AI models"),
  row("FIREWORKS_API_KEY", "Fireworks key", "AI models"),
  row("CEREBRAS_API_KEY", "Cerebras key", "AI models"),
  row("PERPLEXITY_API_KEY", "Perplexity key", "AI models"),
  row("MINIMAX_API_KEY", "MiniMax key", "AI models"),
  row("NVIDIA_API_KEY", "NVIDIA NIM key", "AI models"),
  row("SAMBANOVA_API_KEY", "SambaNova key", "AI models"),
  row("HYPERBOLIC_API_KEY", "Hyperbolic key", "AI models"),
  row("NOVITA_API_KEY", "Novita AI key", "AI models"),
  row("DEEPINFRA_API_KEY", "DeepInfra key", "AI models"),
  row("SILICONFLOW_API_KEY", "SiliconFlow key", "AI models"),
  row("NEBIUS_API_KEY", "Nebius AI Studio key", "AI models"),
  row("CHUTES_API_KEY", "Chutes key", "AI models"),
  row("VENICE_API_KEY", "Venice AI key", "AI models"),
  row("COHERE_API_KEY", "Cohere key", "AI models"),
  row("GITHUB_TOKEN", "GitHub Models token (PAT)", "AI models"),
  row("LMSTUDIO_API_KEY", "LM Studio key (usually none)", "Local models"),
  row("LLAMACPP_API_KEY", "llama.cpp server key (usually none)", "Local models"),
  row("VLLM_API_KEY", "vLLM key (usually none)", "Local models"),
  row("LITELLM_API_KEY", "LiteLLM proxy key (usually none)", "Local models"),
  row(
    "REGENT_SEARCH_PROVIDER",
    "provider (brave|tavily|serpapi|exa|google_cse|duckduckgo)",
    "Search",
  ),
  row("REGENT_SEARCH_API_KEY", "generic key for the selected provider", "Search"),
  row("BRAVE_API_KEY", "Brave Search key", "Search"),
  row("TAVILY_API_KEY", "Tavily key", "Search"),
  row("SERPAPI_API_KEY", "SerpAPI key", "Search"),
  row("EXA_API_KEY", "Exa key", "Search"),
  row("GOOGLE_CSE_API_KEY", "Google CSE key", "Search"),
  row("GOOGLE_CSE_CX", "Google CSE engine id (cx)", "Search"),
  row("REGENT_TELEGRAM_TOKEN", "Telegram bot token", "Messaging"),
  row("REGENT_TELEGRAM_ALLOWED_USERS", "Telegram allowed user ids (comma-sep)", "Messaging"),
  row("REGENT_DISCORD_TOKEN", "Discord bot token", "Messaging"),
  row("DISCORD_PUBLIC_KEY", "Discord interactions public key", "Messaging"),
  row("SLACK_BOT_TOKEN", "Slack bot token", "Messaging"),
  row("SLACK_SIGNING_SECRET", "Slack signing secret", "Messaging"),
  row("WHATSAPP_ACCESS_TOKEN", "WhatsApp access token", "Messaging"),
  row("WHATSAPP_APP_SECRET", "WhatsApp app secret", "Messaging"),
  row("WHATSAPP_PHONE_NUMBER_ID", "WhatsApp phone number id", "Messaging"),
  row("MESSENGER_PAGE_TOKEN", "Messenger page token", "Messaging"),
  row("MESSENGER_APP_SECRET", "Messenger app secret", "Messaging"),
  row("LINE_CHANNEL_ACCESS_TOKEN", "LINE channel access token", "Messaging"),
  row("LINE_CHANNEL_SECRET", "LINE channel secret", "Messaging"),
  row("MATTERMOST_URL", "Mattermost server URL", "Messaging"),
  row("MATTERMOST_BOT_TOKEN", "Mattermost bot token", "Messaging"),
  row("MATTERMOST_VERIFY_TOKEN", "Mattermost outgoing-webhook verify token", "Messaging"),
  row("TWILIO_ACCOUNT_SID", "Twilio account SID", "Messaging"),
  row("TWILIO_AUTH_TOKEN", "Twilio auth token", "Messaging"),
  row("TWILIO_FROM_NUMBER", "Twilio from number", "Messaging"),
  row("TWILIO_VOICE_GREETING", "Twilio voice greeting (enables phone calls)", "Messaging"),
  row("TEAMS_OUTGOING_SECRET", "Teams outgoing-webhook secret", "Messaging"),
  row("FEISHU_VERIFICATION_TOKEN", "Feishu verification token", "Messaging"),
  row("FEISHU_ENCRYPT_KEY", "Feishu encrypt key", "Messaging"),
  row("FEISHU_TENANT_TOKEN", "Feishu tenant access token", "Messaging"),
  row("WECHAT_TOKEN", "WeChat token", "Messaging"),
  row("WECHAT_ENCODING_AES_KEY", "WeChat encoding AES key", "Messaging"),
  row("WECHAT_ACCESS_TOKEN", "WeChat access token", "Messaging"),
  row("WECOM_TOKEN", "WeCom token", "Messaging"),
  row("WECOM_ENCODING_AES_KEY", "WeCom encoding AES key", "Messaging"),
  row("WECOM_ACCESS_TOKEN", "WeCom access token", "Messaging"),
  row("WECOM_AGENT_ID", "WeCom agent id", "Messaging"),
  row("MAILGUN_API_KEY", "Mailgun API key", "Messaging"),
  row("MAILGUN_SIGNING_KEY", "Mailgun webhook signing key", "Messaging"),
  row("MAILGUN_DOMAIN", "Mailgun domain", "Messaging"),
  row("MAILGUN_FROM", "Mailgun from address", "Messaging"),
  row("JIRA_EMAIL", "Jira account email", "Messaging"),
  row("JIRA_API_TOKEN", "Jira API token", "Messaging"),
  row("JIRA_BASE_URL", "Jira base URL", "Messaging"),
  row("JIRA_WEBHOOK_SECRET", "Jira webhook secret", "Messaging"),
  row("AZURE_DEVOPS_PAT", "Azure DevOps PAT", "Messaging"),
  row("AZURE_DEVOPS_ORG_URL", "Azure DevOps org URL", "Messaging"),
  row("AZURE_DEVOPS_BASIC_USER", "Azure DevOps webhook basic-auth user", "Messaging"),
  row("AZURE_DEVOPS_BASIC_PASS", "Azure DevOps webhook basic-auth password", "Messaging"),
  row("TRELLO_API_KEY", "Trello API key", "Messaging"),
  row("TRELLO_API_SECRET", "Trello API secret", "Messaging"),
  row("TRELLO_TOKEN", "Trello token", "Messaging"),
  row("GCHAT_AUDIENCE", "Google Chat audience (project number)", "Messaging"),
  row("REGENT_SPEECH_PROVIDER", "provider for voice calls", "Speech"),
  row("REGENT_SPEECH_API_KEY", "generic speech key", "Speech"),
  row("LEMONFOX_API_KEY", "Lemonfox speech key", "Speech"),
  row("AIMLAPI_API_KEY", "AI/ML API key", "Speech"),
  row("AZURE_OPENAI_API_KEY", "Azure OpenAI key", "Speech"),
  row("RUNPOD_API_KEY", "RunPod key", "Speech"),
  row("REGENT_VISION_API_KEY", "generic image-analysis key", "Vision"),
  row("REGENT_IMAGE_API_KEY", "generic OpenAI-compatible image key", "Image generation"),
  row("STABILITY_API_KEY", "Stability AI key", "Image generation"),
  row("REPLICATE_API_TOKEN", "Replicate token", "Image generation"),
  row("FAL_KEY", "fal.ai key", "Image generation"),
  row("LEONARDO_API_KEY", "Leonardo.Ai key (Regent slot)", "Image generation"),
  row("IDEOGRAM_API_KEY", "Ideogram key (Regent slot)", "Image generation"),
  row("BFL_API_KEY", "Black Forest Labs (FLUX) key", "Image generation"),
  row("RECRAFT_API_TOKEN", "Recraft token", "Image generation"),
  row("CLIPDROP_API_KEY", "Clipdrop key (Regent slot)", "Image generation"),
  row("SEGMIND_API_KEY", "Segmind key", "Image generation"),
  row("DEEPAI_API_KEY", "DeepAI key (Regent slot)", "Image generation"),
  row("RUNWAYML_API_SECRET", "Runway API secret", "Video generation"),
  row("LUMAAI_API_KEY", "Luma Dream Machine key", "Video generation"),
  row("KLING_API_KEY", "Kling key (Regent slot; also image)", "Video generation"),
  row("PIKA_API_KEY", "Pika key", "Video generation"),
  row("HAIPER_API_KEY", "Haiper key (Regent slot)", "Video generation"),
  row("HEYGEN_API_KEY", "HeyGen key", "Video generation"),
  row("SYNTHESIA_API_KEY", "Synthesia key (Regent slot)", "Video generation"),
  row("DID_API_KEY", "D-ID key (Regent slot)", "Video generation"),
  row("TAVUS_API_KEY", "Tavus key", "Video generation"),
  row("VIDU_API_KEY", "Vidu key", "Video generation"),
  row(
    "REGENT_BROWSER_MCP_URL",
    "Playwright-compatible browser automation endpoint",
    "Integrations",
  ),
];

/** Exact mutation boundary for `regent keys`; runtime flags are never inferred. */
export const SETTABLE_KEYS: ReadonlySet<string> = new Set(MANAGED_KEYS.map((row) => row.env));

/** Desktop and the deacon expose canonical secondary slots `_2` through `_8`. */
/**
 * Canonical slots only. `[0-9]+` with `Number()` accepted `KEY_02` (and
 * `KEY_002`), matching Rust's old `parse::<u8>()`. Both were settable and
 * neither was ever listed or resolved, because every enumerator writes `_2`
 * through `_8` literally — a credential saved into a hole. One spelling per
 * slot, on both sides.
 */
export function isSettableKey(name: string): boolean {
  if (SETTABLE_KEYS.has(name)) return true;
  const match = /^(.*)_([2-9])$/.exec(name);
  if (match === null) return false;
  const slot = Number(match[2]);
  return slot >= 2 && slot <= MAX_KEY_SLOTS && SETTABLE_KEYS.has(match[1] ?? "");
}
