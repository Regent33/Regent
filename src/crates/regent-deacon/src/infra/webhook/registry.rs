//! Adapter + file-sender construction from environment secrets. Split out of the
//! webhook ingress module so each file stays focused. A platform registers only
//! when all of its secrets are present in the env.

use regent_gateway::{
    FeishuAdapter, LineAdapter, MattermostAdapter, MessengerAdapter, SlackAdapter, TeamsAdapter,
    TwilioSmsAdapter, TwilioVoiceAdapter, WeChatAdapter, WeComAdapter, WebhookAdapter,
    WebhookFileSender, WhatsAppAdapter,
};
use std::collections::HashMap;
use std::sync::Arc;

/// Platform name → verified webhook adapter.
pub(crate) type Registry = HashMap<String, Arc<dyn WebhookAdapter>>;

/// Builds the adapter registry from environment secrets. A platform is enabled
/// only when all of its secrets are set. `spawn_refreshers` starts background
/// key-refresh tasks (Google Chat JWKS) that are needed only for *inbound*
/// verification — the outbound delivery registry passes `false` so it never
/// spawns a duplicate task.
fn build_registry(spawn_refreshers: bool) -> Registry {
    let mut reg = Registry::new();
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());

    if let (Some(s), Some(t)) = (var("SLACK_SIGNING_SECRET"), var("SLACK_BOT_TOKEN")) {
        reg.insert("slack".to_owned(), Arc::new(SlackAdapter::new(s, t)));
    }
    if let (Some(s), Some(t)) = (var("MESSENGER_APP_SECRET"), var("MESSENGER_PAGE_TOKEN")) {
        reg.insert(
            "messenger".to_owned(),
            Arc::new(MessengerAdapter::new(s, t)),
        );
    }
    if let (Some(s), Some(t)) = (var("LINE_CHANNEL_SECRET"), var("LINE_CHANNEL_ACCESS_TOKEN")) {
        reg.insert("line".to_owned(), Arc::new(LineAdapter::new(s, t)));
    }
    if let (Some(s), Some(t), Some(p)) = (
        var("WHATSAPP_APP_SECRET"),
        var("WHATSAPP_ACCESS_TOKEN"),
        var("WHATSAPP_PHONE_NUMBER_ID"),
    ) {
        reg.insert(
            "whatsapp".to_owned(),
            Arc::new(WhatsAppAdapter::new(s, t, p)),
        );
    }
    if let (Some(u), Some(v), Some(b)) = (
        var("MATTERMOST_URL"),
        var("MATTERMOST_VERIFY_TOKEN"),
        var("MATTERMOST_BOT_TOKEN"),
    ) {
        reg.insert(
            "mattermost".to_owned(),
            Arc::new(MattermostAdapter::new(u, v, b)),
        );
    }
    if let (Some(sid), Some(tok), Some(from)) = (
        var("TWILIO_ACCOUNT_SID"),
        var("TWILIO_AUTH_TOKEN"),
        var("TWILIO_FROM_NUMBER"),
    ) {
        reg.insert(
            "twilio_sms".to_owned(),
            Arc::new(TwilioSmsAdapter::new(sid, tok, from)),
        );
    }
    if let Some(secret) = var("TEAMS_OUTGOING_SECRET") {
        reg.insert("teams".to_owned(), Arc::new(TeamsAdapter::new(secret)));
    }
    // Voice reuses the Twilio auth token; the greeting's presence enables it.
    if let (Some(tok), Some(greeting)) = (var("TWILIO_AUTH_TOKEN"), var("TWILIO_VOICE_GREETING")) {
        reg.insert(
            "twilio_voice".to_owned(),
            Arc::new(TwilioVoiceAdapter::new(tok, greeting)),
        );
    }
    super::registry_ext::register_asia_and_work_platforms(&mut reg, spawn_refreshers);
    reg
}

/// Inbound adapter registry (verifies requests). Spawns background key-refresh
/// tasks where an adapter needs them (Google Chat JWKS).
#[must_use]
pub fn registry_from_env() -> Registry {
    build_registry(true)
}

/// Outbound-only registry for reply delivery. It never verifies inbound
/// requests, so it must not spawn a second Google Chat JWKS refresher.
#[must_use]
pub(crate) fn delivery_registry_from_env() -> Registry {
    build_registry(false)
}

/// File-upload adapters keyed by platform, from the same env as
/// [`registry_from_env`]. Only platforms with an upload path register; the rest
/// simply have no file sender (and `send_file` declines for them).
#[must_use]
pub fn file_senders_from_env() -> HashMap<String, Arc<dyn WebhookFileSender>> {
    let mut senders: HashMap<String, Arc<dyn WebhookFileSender>> = HashMap::new();
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    if let (Some(s), Some(t), Some(p)) = (
        var("WHATSAPP_APP_SECRET"),
        var("WHATSAPP_ACCESS_TOKEN"),
        var("WHATSAPP_PHONE_NUMBER_ID"),
    ) {
        senders.insert(
            "whatsapp".to_owned(),
            Arc::new(WhatsAppAdapter::new(s, t, p)),
        );
    }
    if let (Some(s), Some(t)) = (var("SLACK_SIGNING_SECRET"), var("SLACK_BOT_TOKEN")) {
        senders.insert("slack".to_owned(), Arc::new(SlackAdapter::new(s, t)));
    }
    // WeChat media send needs the operator access token (the verify token alone
    // can't call the media/upload + custom/send APIs).
    if let (Some(token), Some(access)) = (var("WECHAT_TOKEN"), var("WECHAT_ACCESS_TOKEN")) {
        senders.insert(
            "wechat".to_owned(),
            Arc::new(WeChatAdapter::new(
                token,
                var("WECHAT_ENCODING_AES_KEY"),
                Some(access),
            )),
        );
    }
    if let (Some(url), Some(bot)) = (var("MATTERMOST_URL"), var("MATTERMOST_BOT_TOKEN")) {
        senders.insert(
            "mattermost".to_owned(),
            Arc::new(MattermostAdapter::new(
                url,
                var("MATTERMOST_VERIFY_TOKEN").unwrap_or_default(),
                bot,
            )),
        );
    }
    if let (Some(secret), Some(token)) = (var("MESSENGER_APP_SECRET"), var("MESSENGER_PAGE_TOKEN"))
    {
        senders.insert(
            "messenger".to_owned(),
            Arc::new(MessengerAdapter::new(secret, token)),
        );
    }
    // Feishu file send needs the tenant access token (the verification token alone
    // can't call im/v1/files + im/v1/messages).
    if let (Some(vtoken), Some(tenant)) =
        (var("FEISHU_VERIFICATION_TOKEN"), var("FEISHU_TENANT_TOKEN"))
    {
        senders.insert(
            "feishu".to_owned(),
            Arc::new(FeishuAdapter::new(
                vtoken,
                var("FEISHU_ENCRYPT_KEY"),
                Some(tenant),
            )),
        );
    }
    // WeCom file send needs the operator access token + agent id.
    if let (Some(token), Some(access), Some(agent)) = (
        var("WECOM_TOKEN"),
        var("WECOM_ACCESS_TOKEN"),
        var("WECOM_AGENT_ID"),
    ) {
        senders.insert(
            "wecom".to_owned(),
            Arc::new(WeComAdapter::new(
                token,
                var("WECOM_ENCODING_AES_KEY").unwrap_or_default(),
                Some(access),
                agent,
            )),
        );
    }
    senders
}
