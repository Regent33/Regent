//! The managed-key catalog: which env vars `manage_keys` advertises, which are
//! protected, and how each buckets into a UI group.

/// Provider keys the tool advertises and permits (plus canonical _2 through _8
/// numbered slots derived from this exact list).
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
    // The deacon's registry gates the Twilio *voice* adapter on this being set
    // (`TWILIO_AUTH_TOKEN` + a greeting). Missing here, phone calls could not be
    // turned on through any supported path — the runtime read a name no writer
    // would accept.
    (
        "TWILIO_VOICE_GREETING",
        "Twilio voice greeting (enables phone calls)",
    ),
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
    // Read by the registry to verify Azure DevOps' basic-auth service hooks
    // (the PAT alone only covers the outbound REST calls).
    (
        "AZURE_DEVOPS_BASIC_USER",
        "Azure DevOps webhook basic-auth user",
    ),
    (
        "AZURE_DEVOPS_BASIC_PASS",
        "Azure DevOps webhook basic-auth password",
    ),
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
    // Image-generation credentials. These rows are credential management,
    // not a claim that every provider shares Regent's OpenAI-compatible
    // `image_generation` wire format.
    ("STABILITY_API_KEY", "Stability AI key"),
    ("REPLICATE_API_TOKEN", "Replicate token"),
    ("FAL_KEY", "fal.ai key"),
    ("LEONARDO_API_KEY", "Leonardo.Ai key (Regent slot)"),
    ("IDEOGRAM_API_KEY", "Ideogram key (Regent slot)"),
    ("BFL_API_KEY", "Black Forest Labs (FLUX) key"),
    ("RECRAFT_API_TOKEN", "Recraft token"),
    ("CLIPDROP_API_KEY", "Clipdrop key (Regent slot)"),
    ("SEGMIND_API_KEY", "Segmind key"),
    ("DEEPAI_API_KEY", "DeepAI key (Regent slot)"),
    // Video-generation credentials. Kling, Luma, and Haiper also offer image
    // products, so `extra_key_groups` exposes the same stored key in both
    // panels without duplicating the secret.
    ("RUNWAYML_API_SECRET", "Runway API secret"),
    ("LUMAAI_API_KEY", "Luma Dream Machine key"),
    ("KLING_API_KEY", "Kling key (Regent slot)"),
    ("PIKA_API_KEY", "Pika key"),
    ("HAIPER_API_KEY", "Haiper key (Regent slot)"),
    ("HEYGEN_API_KEY", "HeyGen key"),
    ("SYNTHESIA_API_KEY", "Synthesia key (Regent slot)"),
    ("DID_API_KEY", "D-ID key (Regent slot)"),
    ("TAVUS_API_KEY", "Tavus key"),
    ("VIDU_API_KEY", "Vidu key"),
    // Automation endpoint, not a provider secret — but `status_ops` tells the
    // user to store it with `regent keys set REGENT_BROWSER_MCP_URL --stdin`,
    // and `browser.rs` reads it. Absent from this list, that instruction named
    // a variable the agent's own key tool refused and the API Keys page never
    // showed: the CLI mirror accepted it, so the two surfaces disagreed.
    (
        "REGENT_BROWSER_MCP_URL",
        "browser automation endpoint (Playwright MCP)",
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

pub(super) const MAX_KEY_SLOTS: u8 = 8;

/// The slot number in a `KEY_N` name, only for the CANONICAL spelling.
///
/// `"02".parse::<u8>()` is `Ok(2)`, so a plain parse accepted `KEY_02`,
/// `KEY_002` and so on. Those were settable but invisible: the listing
/// enumerates `_2..=_8` literally, so a key stored in a padded slot never
/// appeared in the CLI or the app, and nothing resolved it at runtime — a
/// credential saved into a hole. One spelling per slot, rejected at the
/// boundary rather than tolerated and then ignored.
#[must_use]
pub(super) fn canonical_slot(suffix: &str) -> Option<u8> {
    let [digit] = suffix.as_bytes() else {
        return None;
    };
    let slot = digit.checked_sub(b'0')?;
    (2..=MAX_KEY_SLOTS).contains(&slot).then_some(slot)
}

/// Whether the agent-facing key tool may mutate this exact variable. Runtime
/// flags are intentionally absent: a syntactically credential-looking name is
/// not authority to rewrite the deacon's behaviour.
#[must_use]
pub(super) fn is_managed_key(name: &str) -> bool {
    if MANAGED.iter().any(|(managed, _)| *managed == name) {
        return true;
    }
    let Some((base, suffix)) = name.rsplit_once('_') else {
        return false;
    };
    canonical_slot(suffix).is_some() && MANAGED.iter().any(|(managed, _)| *managed == base)
}

/// Extra UI groups a key also belongs to beyond [`key_group`]'s primary one.
/// Some providers sell both image and video products under one account; the
/// UI therefore shows each provider's one stored variable in both places.
#[must_use]
pub fn extra_key_groups(name: &str) -> &'static [&'static str] {
    match name {
        // Each account/key covers both product APIs. Keep one stored secret,
        // but show it anywhere a user would reasonably look for it.
        "KLING_API_KEY" | "LUMAAI_API_KEY" | "HAIPER_API_KEY" => &["image"],
        _ => &[],
    }
}

/// Classify a managed key into a UI group for the API Keys page:
/// `"llm" | "local" | "messaging" | "search" | "speech" | "vision" |
/// "image" | "video" | "integrations"`.
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
        "LUMAAI",
        "KLING",
        "PIKA_",
        "HAIPER",
        "HEYGEN",
        "SYNTHESIA",
        "DID_",
        "TAVUS",
        "VIDU_",
    ];
    const LOCAL: &[&str] = &["OLLAMA", "LMSTUDIO", "LLAMACPP", "VLLM", "LITELLM"];
    if name == "REGENT_IMAGE_API_KEY" {
        return "image";
    }
    if name == "REGENT_VISION_API_KEY" {
        return "vision";
    }
    if name == "REGENT_BROWSER_MCP_URL" {
        return "integrations";
    }
    if IMAGE.iter().any(|part| name.contains(part)) {
        return "image";
    }
    if VIDEO.iter().any(|part| name.contains(part)) {
        return "video";
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
    // AIMLAPI / AZURE_OPENAI / RUNPOD sit under this file's own "Speech
    // (ASR/TTS) providers" comment in MANAGED, but the classifier did not list
    // them — so the page filed them under LLM providers, contradicting the
    // comment that put them there. `AZURE_OPENAI` cannot collide with
    // `AZURE_DEVOPS`: messaging is matched first and neither prefix is a
    // substring of the other.
    const SPEECH: &[&str] = &["SPEECH", "LEMONFOX", "AIMLAPI", "AZURE_OPENAI", "RUNPOD"];
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
