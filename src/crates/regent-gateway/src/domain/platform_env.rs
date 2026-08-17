//! The messaging-credential name contract: for each platform, the exact `.env`
//! variable names that enable it at runtime.
//!
//! Three surfaces have to agree on these strings and none of them shares a
//! type with the others:
//!
//! - the **writers** — the managed key catalog ([`regent_tools::MANAGED`] and
//!   its TypeScript mirror `keyCatalog.ts`), which decide what `regent keys
//!   set`, the agent's key tool, and the Desktop API Keys page will accept;
//! - the **readers** — the deacon's `infra/webhook/registry.rs` and
//!   `registry_ext.rs`, which build an adapter only when *all* of a platform's
//!   secrets are present, plus `REGENT_TELEGRAM_TOKEN` (the gateway binary) and
//!   `DISCORD_PUBLIC_KEY` (the deacon's interactions route).
//!
//! A name that only one side knows is silent: the credential saves, the page
//! shows it set, and the platform never turns on. This table is the contract,
//! and `platform_env_tests.rs` diffs it against both ends.
//!
//! Optional-but-read names are listed too (e.g. `FEISHU_ENCRYPT_KEY`), because
//! "settable" is what the contract is about — not "required".

/// Platform id → every env var name its runtime path reads. Platform ids match
/// the keys the deacon's registry inserts (`/webhook/{platform}`).
pub const PLATFORM_CREDENTIALS: &[(&str, &[&str])] = &[
    (
        "telegram",
        &["REGENT_TELEGRAM_TOKEN", "REGENT_TELEGRAM_ALLOWED_USERS"],
    ),
    ("discord", &["DISCORD_PUBLIC_KEY", "REGENT_DISCORD_TOKEN"]),
    ("slack", &["SLACK_SIGNING_SECRET", "SLACK_BOT_TOKEN"]),
    (
        "whatsapp",
        &[
            "WHATSAPP_APP_SECRET",
            "WHATSAPP_ACCESS_TOKEN",
            "WHATSAPP_PHONE_NUMBER_ID",
        ],
    ),
    (
        "messenger",
        &["MESSENGER_APP_SECRET", "MESSENGER_PAGE_TOKEN"],
    ),
    (
        "line",
        &["LINE_CHANNEL_SECRET", "LINE_CHANNEL_ACCESS_TOKEN"],
    ),
    (
        "mattermost",
        &[
            "MATTERMOST_URL",
            "MATTERMOST_VERIFY_TOKEN",
            "MATTERMOST_BOT_TOKEN",
        ],
    ),
    // One Twilio account serves both adapters: SMS needs the SID/from-number,
    // voice reuses the auth token and is gated on the greeting being set.
    (
        "twilio",
        &[
            "TWILIO_ACCOUNT_SID",
            "TWILIO_AUTH_TOKEN",
            "TWILIO_FROM_NUMBER",
            "TWILIO_VOICE_GREETING",
        ],
    ),
    ("teams", &["TEAMS_OUTGOING_SECRET"]),
    (
        "feishu",
        &[
            "FEISHU_VERIFICATION_TOKEN",
            "FEISHU_ENCRYPT_KEY",
            "FEISHU_TENANT_TOKEN",
        ],
    ),
    (
        "wechat",
        &[
            "WECHAT_TOKEN",
            "WECHAT_ENCODING_AES_KEY",
            "WECHAT_ACCESS_TOKEN",
        ],
    ),
    (
        "wecom",
        &[
            "WECOM_TOKEN",
            "WECOM_ENCODING_AES_KEY",
            "WECOM_ACCESS_TOKEN",
            "WECOM_AGENT_ID",
        ],
    ),
    (
        "email",
        &[
            "MAILGUN_SIGNING_KEY",
            "MAILGUN_API_KEY",
            "MAILGUN_DOMAIN",
            "MAILGUN_FROM",
        ],
    ),
    (
        "jira",
        &[
            "JIRA_EMAIL",
            "JIRA_API_TOKEN",
            "JIRA_BASE_URL",
            "JIRA_WEBHOOK_SECRET",
        ],
    ),
    (
        "azure_devops",
        &[
            "AZURE_DEVOPS_PAT",
            "AZURE_DEVOPS_ORG_URL",
            "AZURE_DEVOPS_BASIC_USER",
            "AZURE_DEVOPS_BASIC_PASS",
        ],
    ),
    (
        "trello",
        &["TRELLO_API_SECRET", "TRELLO_API_KEY", "TRELLO_TOKEN"],
    ),
    ("google_chat", &["GCHAT_AUDIENCE"]),
];

/// Names the catalog offers that no runtime path reads yet, with the reason.
/// Kept settable on purpose (`regent gateway setup discord` already writes the
/// token, and `DiscordGateway` is built and exported), but the contract test
/// must not pretend they are wired — list them here or the test fails.
pub const UNREAD_BY_RUNTIME: &[(&str, &str)] = &[(
    "REGENT_DISCORD_TOKEN",
    "written by `regent gateway setup discord`; DiscordGateway is exported but \
     never constructed, so no runtime path reads it (chat runs over \
     DISCORD_PUBLIC_KEY interactions)",
)];

/// Auth posture read by the gateway but deliberately absent from the managed
/// catalog: these widen or set who may talk to the agent, so the agent's own
/// key tool must not be able to write them.
pub const NOT_SETTABLE_BY_DESIGN: &[&str] = &[
    "REGENT_ALLOW_ALL",
    "REGENT_TELEGRAM_ALLOW_ALL",
    "REGENT_ALLOWED_USERS",
];

#[cfg(test)]
#[path = "platform_env_tests.rs"]
mod tests;
