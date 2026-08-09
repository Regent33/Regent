//! The managed-key catalog: which env vars `manage_keys` advertises, which are
//! protected, and how each buckets into a UI group.

/// Provider keys the tool advertises in `list` (others are still settable).
/// Public so the deacon's `env.list` surfaces the same managed set (tagging
/// each with [`key_group`]); the shape stays `(var, label)` for the tool.
pub const MANAGED: &[(&str, &str)] = &[
    // LLM provider keys (the main model). REGENT_API_KEY is the generic
    // fallback but stays PROTECTED below (can't be set via this tool); the
    // provider-specific keys here are settable and preferred.
    ("ANTHROPIC_API_KEY", "Anthropic key"),
    ("OPENAI_API_KEY", "OpenAI key"),
    ("OPENROUTER_API_KEY", "OpenRouter key"),
    ("GROQ_API_KEY", "Groq key"),
    ("DEEPSEEK_API_KEY", "DeepSeek key"),
    ("TOGETHER_API_KEY", "Together key"),
    ("OLLAMA_API_KEY", "Ollama key (Ollama Cloud)"),
    ("MISTRAL_API_KEY", "Mistral key"),
    ("XAI_API_KEY", "xAI (Grok) key"),
    ("GEMINI_API_KEY", "Google Gemini key"),
    ("MOONSHOT_API_KEY", "Moonshot (Kimi) key"),
    ("ZHIPU_API_KEY", "Zhipu (GLM/Z.AI) key"),
    ("DASHSCOPE_API_KEY", "DashScope (Qwen) key"),
    ("FIREWORKS_API_KEY", "Fireworks key"),
    ("CEREBRAS_API_KEY", "Cerebras key"),
    ("PERPLEXITY_API_KEY", "Perplexity key"),
    ("MINIMAX_API_KEY", "MiniMax key"),
    ("NVIDIA_API_KEY", "NVIDIA NIM key"),
    // Open-weights hosts — same OpenAI-compatible wire, HF-style model ids.
    ("SAMBANOVA_API_KEY", "SambaNova key"),
    ("HYPERBOLIC_API_KEY", "Hyperbolic key"),
    ("NOVITA_API_KEY", "Novita AI key"),
    ("DEEPINFRA_API_KEY", "DeepInfra key"),
    ("SILICONFLOW_API_KEY", "SiliconFlow key"),
    ("NEBIUS_API_KEY", "Nebius AI Studio key"),
    ("CHUTES_API_KEY", "Chutes key"),
    ("VENICE_API_KEY", "Venice AI key"),
    ("COHERE_API_KEY", "Cohere key"),
    ("GITHUB_TOKEN", "GitHub Models token (PAT)"),
    // Servers you run yourself: usually keyless, listed so an authenticated
    // proxy (LiteLLM master key, a locked-down vLLM) has somewhere to put one.
    ("LMSTUDIO_API_KEY", "LM Studio key (usually none)"),
    ("LLAMACPP_API_KEY", "llama.cpp server key (usually none)"),
    ("VLLM_API_KEY", "vLLM key (usually none)"),
    ("LITELLM_API_KEY", "LiteLLM proxy key (usually none)"),
    (
        "REGENT_SEARCH_PROVIDER",
        "search provider (brave|tavily|serpapi|exa|google_cse|duckduckgo)",
    ),
    ("REGENT_SEARCH_API_KEY", "search key (generic fallback)"),
    ("BRAVE_API_KEY", "Brave Search key"),
    ("TAVILY_API_KEY", "Tavily key"),
    ("SERPAPI_API_KEY", "SerpAPI key"),
    ("EXA_API_KEY", "Exa key"),
    ("GOOGLE_CSE_API_KEY", "Google CSE key"),
    ("GOOGLE_CSE_CX", "Google CSE engine id (cx)"),
    ("REGENT_TELEGRAM_TOKEN", "Telegram bot token"),
    (
        "REGENT_TELEGRAM_ALLOWED_USERS",
        "Telegram allowed user ids (comma-sep)",
    ),
    ("REGENT_DISCORD_TOKEN", "Discord bot token"),
    ("DISCORD_PUBLIC_KEY", "Discord interactions public key"),
    ("SLACK_BOT_TOKEN", "Slack bot token"),
    ("SLACK_SIGNING_SECRET", "Slack signing secret"),
    ("WHATSAPP_ACCESS_TOKEN", "WhatsApp access token"),
    ("WHATSAPP_APP_SECRET", "WhatsApp app secret"),
    ("WHATSAPP_PHONE_NUMBER_ID", "WhatsApp phone number id"),
    ("MESSENGER_PAGE_TOKEN", "Messenger page token"),
    ("MESSENGER_APP_SECRET", "Messenger app secret"),
    ("LINE_CHANNEL_ACCESS_TOKEN", "LINE channel access token"),
    ("LINE_CHANNEL_SECRET", "LINE channel secret"),
    ("MATTERMOST_URL", "Mattermost server URL"),
    ("MATTERMOST_BOT_TOKEN", "Mattermost bot token"),
    (
        "MATTERMOST_VERIFY_TOKEN",
        "Mattermost outgoing-webhook verify token",
    ),
    ("TWILIO_ACCOUNT_SID", "Twilio account SID"),
    ("TWILIO_AUTH_TOKEN", "Twilio auth token"),
    ("TWILIO_FROM_NUMBER", "Twilio from number"),
    ("TEAMS_OUTGOING_SECRET", "Teams outgoing-webhook secret"),
    ("FEISHU_VERIFICATION_TOKEN", "Feishu verification token"),
    ("FEISHU_ENCRYPT_KEY", "Feishu encrypt key"),
    ("FEISHU_TENANT_TOKEN", "Feishu tenant access token"),
    ("WECHAT_TOKEN", "WeChat token"),
    ("WECHAT_ENCODING_AES_KEY", "WeChat encoding AES key"),
    ("WECHAT_ACCESS_TOKEN", "WeChat access token"),
    ("WECOM_TOKEN", "WeCom token"),
    ("WECOM_ENCODING_AES_KEY", "WeCom encoding AES key"),
    ("WECOM_ACCESS_TOKEN", "WeCom access token"),
    ("WECOM_AGENT_ID", "WeCom agent id"),
    ("MAILGUN_API_KEY", "Mailgun API key"),
    ("MAILGUN_SIGNING_KEY", "Mailgun webhook signing key"),
    ("MAILGUN_DOMAIN", "Mailgun domain"),
    ("MAILGUN_FROM", "Mailgun from address"),
    ("JIRA_EMAIL", "Jira account email"),
    ("JIRA_API_TOKEN", "Jira API token"),
    ("JIRA_BASE_URL", "Jira base URL"),
    ("JIRA_WEBHOOK_SECRET", "Jira webhook secret"),
    ("AZURE_DEVOPS_PAT", "Azure DevOps PAT"),
    ("AZURE_DEVOPS_ORG_URL", "Azure DevOps org URL"),
    ("TRELLO_API_KEY", "Trello API key"),
    ("TRELLO_API_SECRET", "Trello API secret"),
    ("TRELLO_TOKEN", "Trello token"),
    ("GCHAT_AUDIENCE", "Google Chat audience (project number)"),
    (
        "REGENT_SPEECH_PROVIDER",
        "speech provider (for voice calls)",
    ),
    ("REGENT_SPEECH_API_KEY", "speech API key (for voice calls)"),
    // Speech (ASR/TTS) providers whose key is not already listed above.
    ("LEMONFOX_API_KEY", "Lemonfox speech key"),
    ("AIMLAPI_API_KEY", "AI/ML API key"),
    ("AZURE_OPENAI_API_KEY", "Azure OpenAI key"),
    ("RUNPOD_API_KEY", "RunPod key"),
    (
        "REGENT_VISION_API_KEY",
        "vision API key (image analysis; falls back to REGENT_API_KEY)",
    ),
    (
        "REGENT_IMAGE_API_KEY",
        "image generation API key (falls back to REGENT_API_KEY)",
    ),
];

/// Never writable here: the AI-model secret + runtime/config vars (avoid the
/// agent clobbering its own model/provider wiring through this tool).
pub(super) const PROTECTED: &[&str] = &[
    "REGENT_API_KEY",
    "REGENT_MODEL",
    "REGENT_BASE_URL",
    "REGENT_PROVIDER",
    "REGENT_HOME",
    "REGENT_NOW",
];

/// Extra UI groups a key also belongs to beyond [`key_group`]'s primary one.
/// The current shipped adapters need no cross-product duplicates; keeping this
/// seam makes a future real multi-product adapter an additive catalog change.
#[must_use]
pub fn extra_key_groups(name: &str) -> &'static [&'static str] {
    let _ = name;
    &[]
}

/// Classify a managed key into a UI group for the API Keys page:
/// `"llm" | "local" | "messaging" | "search" | "speech" | "vision" | "image"`.
/// Matched by name substring so every [`MANAGED`] key (and the generic LLM
/// fallback) buckets deterministically; anything unrecognised falls back to
/// `"llm"` (the flat default).
#[must_use]
pub fn key_group(name: &str) -> &'static str {
    const LOCAL: &[&str] = &["OLLAMA", "LMSTUDIO", "LLAMACPP", "VLLM", "LITELLM"];
    if name == "REGENT_IMAGE_API_KEY" {
        return "image";
    }
    if name == "REGENT_VISION_API_KEY" {
        return "vision";
    }
    if LOCAL.iter().any(|p| name.contains(p)) {
        return "local";
    }
    const MESSAGING: &[&str] = &[
        "TELEGRAM",
        "DISCORD",
        "SLACK",
        "WHATSAPP",
        "MESSENGER",
        "LINE_CHANNEL",
        "MATTERMOST",
        "TWILIO",
        "TEAMS",
        "FEISHU",
        "WECHAT",
        "WECOM",
        "MAILGUN",
        "JIRA",
        "AZURE_DEVOPS",
        "TRELLO",
        "GCHAT",
    ];
    const SEARCH: &[&str] = &["SEARCH", "BRAVE", "TAVILY", "SERPAPI", "EXA_", "GOOGLE_CSE"];
    const SPEECH: &[&str] = &["SPEECH", "LEMONFOX"];
    if MESSAGING.iter().any(|p| name.contains(p)) {
        "messaging"
    } else if SEARCH.iter().any(|p| name.contains(p)) {
        "search"
    } else if SPEECH.iter().any(|p| name.contains(p)) {
        "speech"
    } else {
        "llm"
    }
}
