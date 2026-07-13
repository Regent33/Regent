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
    (
        "REGENT_VISION_API_KEY",
        "vision API key (image analysis; falls back to REGENT_API_KEY)",
    ),
    // Image generation
    ("STABILITY_API_KEY", "Stability AI key"),
    ("REPLICATE_API_TOKEN", "Replicate token"),
    ("FAL_API_KEY", "fal.ai key"),
    ("LEONARDO_API_KEY", "Leonardo.Ai key"),
    ("IDEOGRAM_API_KEY", "Ideogram key"),
    ("BFL_API_KEY", "Black Forest Labs (FLUX) key"),
    ("RECRAFT_API_KEY", "Recraft key"),
    ("CLIPDROP_API_KEY", "Clipdrop key"),
    ("SEGMIND_API_KEY", "Segmind key"),
    ("DEEPAI_API_KEY", "DeepAI key"),
    // Video generation
    ("RUNWAY_API_KEY", "Runway key"),
    ("LUMA_API_KEY", "Luma (Dream Machine) key"),
    ("KLING_API_KEY", "Kling key"),
    ("PIKA_API_KEY", "Pika key"),
    ("HAIPER_API_KEY", "Haiper key"),
    ("HEYGEN_API_KEY", "HeyGen key"),
    ("SYNTHESIA_API_KEY", "Synthesia key"),
    ("DID_API_KEY", "D-ID key"),
    ("TAVUS_API_KEY", "Tavus key"),
    ("VIDU_API_KEY", "Vidu key"),
    ("HIGGSFIELD_API_KEY", "Higgsfield key"),
    // Sound / audio generation
    ("ELEVENLABS_API_KEY", "ElevenLabs key"),
    ("PLAYHT_API_KEY", "Play.ht key"),
    ("SUNO_API_KEY", "Suno key"),
    ("UDIO_API_KEY", "Udio key"),
    ("MURF_API_KEY", "Murf key"),
    ("RESEMBLE_API_KEY", "Resemble AI key"),
    ("CARTESIA_API_KEY", "Cartesia key"),
    ("DEEPGRAM_API_KEY", "Deepgram key"),
    ("ASSEMBLYAI_API_KEY", "AssemblyAI key"),
    ("LOVO_API_KEY", "LOVO key"),
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

/// Extra UI groups a key ALSO belongs to beyond [`key_group`]'s primary one —
/// for providers whose one key serves several generation products (Kling and
/// Higgsfield do video AND photo). `env.list` emits an additional row per
/// extra group; it's the same env var either way.
#[must_use]
pub fn extra_key_groups(name: &str) -> &'static [&'static str] {
    if name.contains("KLING") || name.contains("HIGGSFIELD") {
        &["image"]
    } else {
        &[]
    }
}

/// Classify a managed key into a UI group for the API Keys page:
/// `"llm" | "messaging" | "search" | "speech" | "image" | "video" | "audio"`.
/// Matched by name substring so every [`MANAGED`] key (and the generic LLM
/// fallback) buckets deterministically; anything unrecognised falls back to
/// `"llm"` (the flat default).
#[must_use]
pub fn key_group(name: &str) -> &'static str {
    const IMAGE: &[&str] = &[
        "STABILITY",
        "REPLICATE",
        "FAL_",
        "LEONARDO",
        "IDEOGRAM",
        "BFL_",
        "RECRAFT",
        "CLIPDROP",
        "SEGMIND",
        "DEEPAI",
    ];
    const VIDEO: &[&str] = &[
        "RUNWAY",
        "LUMA_",
        "KLING",
        "PIKA_",
        "HAIPER",
        "HEYGEN",
        "SYNTHESIA",
        "DID_",
        "TAVUS",
        "VIDU_",
        "HIGGSFIELD",
    ];
    const AUDIO: &[&str] = &[
        "ELEVENLABS",
        "PLAYHT",
        "SUNO_",
        "UDIO_",
        "MURF_",
        "RESEMBLE",
        "CARTESIA",
        "DEEPGRAM",
        "ASSEMBLYAI",
        "LOVO_",
    ];
    if IMAGE.iter().any(|p| name.contains(p)) {
        return "image";
    }
    if VIDEO.iter().any(|p| name.contains(p)) {
        return "video";
    }
    if AUDIO.iter().any(|p| name.contains(p)) {
        return "audio";
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
    const SPEECH: &[&str] = &["SPEECH", "VISION"];
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
