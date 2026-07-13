//! Second half of the env-driven adapter registry (file-size rule):
//! Feishu/WeChat/WeCom, email, Jira/Trello/Azure-DevOps, Google Chat.

use super::registry::Registry;
use regent_gateway::{
    AzureDevOpsAdapter, EmailAdapter, FeishuAdapter, GoogleChatAdapter, JiraAdapter, TrelloAdapter,
    WeChatAdapter, WeComAdapter,
};
use std::sync::Arc;

/// Feishu/WeChat/WeCom, email, Jira/Trello/Azure-DevOps, and Google Chat —
/// the second half of the env-driven adapter registry (file-size rule).
#[allow(clippy::too_many_lines)]
pub(in crate::infra::webhook) fn register_asia_and_work_platforms(
    reg: &mut Registry,
    spawn_refreshers: bool,
) {
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    if let Some(token) = var("FEISHU_VERIFICATION_TOKEN") {
        reg.insert(
            "feishu".to_owned(),
            Arc::new(FeishuAdapter::new(
                token,
                var("FEISHU_ENCRYPT_KEY"),
                var("FEISHU_TENANT_TOKEN"),
            )),
        );
    }
    if let Some(token) = var("WECHAT_TOKEN") {
        reg.insert(
            "wechat".to_owned(),
            Arc::new(WeChatAdapter::new(
                token,
                var("WECHAT_ENCODING_AES_KEY"),
                var("WECHAT_ACCESS_TOKEN"),
            )),
        );
    }
    if let (Some(token), Some(aes), Some(agent)) = (
        var("WECOM_TOKEN"),
        var("WECOM_ENCODING_AES_KEY"),
        var("WECOM_AGENT_ID"),
    ) {
        reg.insert(
            "wecom".to_owned(),
            Arc::new(WeComAdapter::new(
                token,
                aes,
                var("WECOM_ACCESS_TOKEN"),
                agent,
            )),
        );
    }
    if let (Some(key), Some(api), Some(domain), Some(from)) = (
        var("MAILGUN_SIGNING_KEY"),
        var("MAILGUN_API_KEY"),
        var("MAILGUN_DOMAIN"),
        var("MAILGUN_FROM"),
    ) {
        reg.insert(
            "email".to_owned(),
            Arc::new(EmailAdapter::new(key, api, domain, from)),
        );
    }
    if let (Some(email), Some(api_token), Some(base)) = (
        var("JIRA_EMAIL"),
        var("JIRA_API_TOKEN"),
        var("JIRA_BASE_URL"),
    ) {
        reg.insert(
            "jira".to_owned(),
            Arc::new(JiraAdapter::new(
                var("JIRA_WEBHOOK_SECRET"),
                email,
                api_token,
                base,
            )),
        );
    }
    if let (Some(pat), Some(org)) = (var("AZURE_DEVOPS_PAT"), var("AZURE_DEVOPS_ORG_URL")) {
        reg.insert(
            "azure_devops".to_owned(),
            Arc::new(AzureDevOpsAdapter::new(
                var("AZURE_DEVOPS_BASIC_USER"),
                var("AZURE_DEVOPS_BASIC_PASS"),
                pat,
                org,
            )),
        );
    }
    if let (Some(secret), Some(key), Some(token)) = (
        var("TRELLO_API_SECRET"),
        var("TRELLO_API_KEY"),
        var("TRELLO_TOKEN"),
    ) {
        reg.insert(
            "trello".to_owned(),
            Arc::new(TrelloAdapter::new(secret, key, token)),
        );
    }
    // Google Chat verifies a Google-signed JWT against rotating JWKS — spawn the
    // background key refresher so `verify` can read the cache synchronously.
    if let Some(audience) = var("GCHAT_AUDIENCE") {
        let adapter = Arc::new(GoogleChatAdapter::new(audience));
        if spawn_refreshers {
            Arc::clone(&adapter).spawn_refresher();
        }
        reg.insert("google_chat".to_owned(), adapter);
    }
}
