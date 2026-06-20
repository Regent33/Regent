//! Platform webhook ingress (P5). One generic `POST /webhook/{platform}` route
//! serves every `WebhookAdapter`: verify the platform signature → parse events →
//! run a turn → deliver the reply via the platform's API. Adapters are built
//! from environment secrets (loaded from `$REGENT_HOME/.env`); only platforms
//! whose secrets are present are registered.
//!
//! The webhook is acknowledged immediately (a 200) and the turn + reply run in
//! the background, the shape push platforms expect.

use crate::infra::http_listener::ChatService;
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use regent_gateway::{
    AzureDevOpsAdapter, EmailAdapter, FeishuAdapter, GoogleChatAdapter, JiraAdapter, LineAdapter,
    MattermostAdapter, MessengerAdapter, OutboundMessage, SendAuth, SendBody, SendRequest,
    SlackAdapter, SyncReply, TeamsAdapter, TrelloAdapter, TwilioSmsAdapter, TwilioVoiceAdapter,
    WeChatAdapter, WeComAdapter, WebhookAdapter, WebhookRequest, WhatsAppAdapter,
};
use std::collections::HashMap;
use std::sync::Arc;

type Registry = HashMap<String, Arc<dyn WebhookAdapter>>;

/// Builds the adapter registry from environment secrets. A platform is enabled
/// only when all of its secrets are set.
#[must_use]
pub fn registry_from_env() -> Registry {
    let mut reg = Registry::new();
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());

    if let (Some(s), Some(t)) = (var("SLACK_SIGNING_SECRET"), var("SLACK_BOT_TOKEN")) {
        reg.insert("slack".to_owned(), Arc::new(SlackAdapter::new(s, t)));
    }
    if let (Some(s), Some(t)) = (var("MESSENGER_APP_SECRET"), var("MESSENGER_PAGE_TOKEN")) {
        reg.insert("messenger".to_owned(), Arc::new(MessengerAdapter::new(s, t)));
    }
    if let (Some(s), Some(t)) = (var("LINE_CHANNEL_SECRET"), var("LINE_CHANNEL_ACCESS_TOKEN")) {
        reg.insert("line".to_owned(), Arc::new(LineAdapter::new(s, t)));
    }
    if let (Some(s), Some(t), Some(p)) = (
        var("WHATSAPP_APP_SECRET"),
        var("WHATSAPP_ACCESS_TOKEN"),
        var("WHATSAPP_PHONE_NUMBER_ID"),
    ) {
        reg.insert("whatsapp".to_owned(), Arc::new(WhatsAppAdapter::new(s, t, p)));
    }
    if let (Some(u), Some(v), Some(b)) = (
        var("MATTERMOST_URL"),
        var("MATTERMOST_VERIFY_TOKEN"),
        var("MATTERMOST_BOT_TOKEN"),
    ) {
        reg.insert("mattermost".to_owned(), Arc::new(MattermostAdapter::new(u, v, b)));
    }
    if let (Some(sid), Some(tok), Some(from)) = (
        var("TWILIO_ACCOUNT_SID"),
        var("TWILIO_AUTH_TOKEN"),
        var("TWILIO_FROM_NUMBER"),
    ) {
        reg.insert("twilio_sms".to_owned(), Arc::new(TwilioSmsAdapter::new(sid, tok, from)));
    }
    if let Some(secret) = var("TEAMS_OUTGOING_SECRET") {
        reg.insert("teams".to_owned(), Arc::new(TeamsAdapter::new(secret)));
    }
    // Voice reuses the Twilio auth token; the greeting's presence enables it.
    if let (Some(tok), Some(greeting)) = (var("TWILIO_AUTH_TOKEN"), var("TWILIO_VOICE_GREETING")) {
        reg.insert("twilio_voice".to_owned(), Arc::new(TwilioVoiceAdapter::new(tok, greeting)));
    }
    if let Some(token) = var("FEISHU_VERIFICATION_TOKEN") {
        reg.insert(
            "feishu".to_owned(),
            Arc::new(FeishuAdapter::new(token, var("FEISHU_ENCRYPT_KEY"), var("FEISHU_TENANT_TOKEN"))),
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
            Arc::new(WeComAdapter::new(token, aes, var("WECOM_ACCESS_TOKEN"), agent)),
        );
    }
    if let (Some(key), Some(api), Some(domain), Some(from)) = (
        var("MAILGUN_SIGNING_KEY"),
        var("MAILGUN_API_KEY"),
        var("MAILGUN_DOMAIN"),
        var("MAILGUN_FROM"),
    ) {
        reg.insert("email".to_owned(), Arc::new(EmailAdapter::new(key, api, domain, from)));
    }
    if let (Some(email), Some(api_token), Some(base)) = (
        var("JIRA_EMAIL"),
        var("JIRA_API_TOKEN"),
        var("JIRA_BASE_URL"),
    ) {
        reg.insert(
            "jira".to_owned(),
            Arc::new(JiraAdapter::new(var("JIRA_WEBHOOK_SECRET"), email, api_token, base)),
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
    if let (Some(secret), Some(key), Some(token)) =
        (var("TRELLO_API_SECRET"), var("TRELLO_API_KEY"), var("TRELLO_TOKEN"))
    {
        reg.insert("trello".to_owned(), Arc::new(TrelloAdapter::new(secret, key, token)));
    }
    // Google Chat verifies a Google-signed JWT against rotating JWKS — spawn the
    // background key refresher so `verify` can read the cache synchronously.
    if let Some(audience) = var("GCHAT_AUDIENCE") {
        let adapter = Arc::new(GoogleChatAdapter::new(audience));
        Arc::clone(&adapter).spawn_refresher();
        reg.insert("google_chat".to_owned(), adapter);
    }
    reg
}

#[derive(Clone)]
struct WebhookState {
    registry: Arc<Registry>,
    service: Arc<dyn ChatService>,
    client: reqwest::Client,
}

/// Router serving `/webhook/{platform}`: `POST` for events, `GET` for the
/// echostr endpoint-verification handshake (WeChat/WeCom).
pub fn router(registry: Registry, service: Arc<dyn ChatService>) -> Router {
    let state = WebhookState { registry: Arc::new(registry), service, client: reqwest::Client::new() };
    Router::new()
        .route("/webhook/{platform}", post(handle).get(handle_get))
        .with_state(state)
}

/// `GET /webhook/{platform}` — the URL-verification handshake. The adapter
/// signs the query and returns the challenge to echo as `text/plain`.
async fn handle_get(
    State(state): State<WebhookState>,
    Path(platform): Path<String>,
    uri: axum::http::Uri,
) -> Response {
    let Some(adapter) = state.registry.get(&platform).cloned() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match adapter.verify_get(uri.query().unwrap_or_default()) {
        Some(echo) => (StatusCode::OK, echo).into_response(),
        None => StatusCode::UNAUTHORIZED.into_response(),
    }
}

async fn handle(
    State(state): State<WebhookState>,
    Path(platform): Path<String>,
    uri: axum::http::Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(adapter) = state.registry.get(&platform).cloned() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let header = |name: Option<&str>| -> Option<String> {
        name.and_then(|n| headers.get(n)).and_then(|v| v.to_str().ok()).map(ToOwned::to_owned)
    };
    let signature = header(adapter.signature_header());
    let timestamp = header(adapter.timestamp_header());
    let nonce = header(adapter.nonce_header());

    // Reconstruct the full public URL (HTTP/1.1 request targets are origin-form,
    // so scheme/host live in proxy headers). Only URL-signing schemes (Twilio)
    // read it; body-only adapters ignore it via the default `verify_request`.
    let scheme =
        header(Some("x-forwarded-proto")).unwrap_or_else(|| "https".to_owned());
    let host = header(Some("x-forwarded-host"))
        .or_else(|| header(Some("host")))
        .unwrap_or_default();
    let path_and_query = uri.path_and_query().map_or_else(|| uri.path(), |pq| pq.as_str());
    let url = format!("{scheme}://{host}{path_and_query}");

    let request = WebhookRequest {
        url: &url,
        body: body.as_ref(),
        signature: signature.as_deref(),
        timestamp: timestamp.as_deref(),
        nonce: nonce.as_deref(),
    };
    if !adapter.verify_request(&request) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    // Endpoint-verification handshake (Feishu/Slack url_verification, WeChat
    // echostr): authenticated above, answered here before any turn runs.
    if let Some(reply) = adapter.handshake(&body) {
        return render_sync(reply);
    }
    let events = match adapter.parse_webhook(&body) {
        Ok(events) => events,
        Err(error) => {
            tracing::warn!(%error, platform, "webhook parse failed");
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    // Sync-response platforms (Teams, Twilio Voice, …) expect the reply in the
    // HTTP response body: run the single turn inline and return it.
    if adapter.sync_reply() {
        let Some(event) = events.into_iter().next() else {
            // No user utterance yet (e.g. Voice's initial call): the adapter may
            // still owe a response (greeting + prompt), else just ack.
            return match adapter.sync_idle_response() {
                Some(reply) => render_sync(reply),
                None => StatusCode::OK.into_response(),
            };
        };
        let key = format!("{platform}:{}", event.chat_id);
        return match state.service.chat_keyed(&key, event.text).await {
            Ok(reply) => render_sync(adapter.sync_response(&reply.reply)),
            Err(error) => {
                tracing::warn!(%error, platform, "webhook turn failed");
                StatusCode::OK.into_response()
            }
        };
    }

    // Otherwise ack fast; run turns + deliver replies off the request path.
    tokio::spawn(async move {
        for event in events {
            // One continuous session per platform conversation.
            let key = format!("{platform}:{}", event.chat_id);
            let reply = match state.service.chat_keyed(&key, event.text).await {
                Ok(reply) => reply.reply,
                Err(error) => {
                    tracing::warn!(%error, platform, "webhook turn failed");
                    continue;
                }
            };
            let out = OutboundMessage { chat_id: event.chat_id, text: reply };
            deliver(&state.client, &adapter.send_request(&out)).await;
        }
    });
    StatusCode::OK.into_response()
}

/// Renders a sync-reply body with the matching `Content-Type` (JSON for
/// Teams/Google Chat, `text/xml` TwiML for Twilio Voice).
fn render_sync(reply: SyncReply) -> Response {
    match reply {
        SyncReply::Json(value) => (StatusCode::OK, Json(value)).into_response(),
        SyncReply::Xml(body) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/xml; charset=utf-8")],
            body,
        )
            .into_response(),
    }
}

async fn deliver(client: &reqwest::Client, req: &SendRequest) {
    let mut builder = match &req.body {
        SendBody::Json(value) => client.post(&req.url).json(value),
        SendBody::Form(pairs) => client.post(&req.url).form(pairs),
    };
    builder = match &req.auth {
        SendAuth::None => builder,
        SendAuth::Bearer(token) => builder.bearer_auth(token),
        SendAuth::Basic { username, password } => builder.basic_auth(username, Some(password)),
    };
    if let Err(error) = builder.send().await {
        tracing::warn!(%error, url = req.url, "webhook reply delivery failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::errors::DaemonError;
    use crate::infra::http_listener::ChatReply;
    use async_trait::async_trait;
    use axum::http::Request;
    use regent_gateway::{GatewayError, MessageEvent};
    use serde_json::json;
    use tower::ServiceExt;

    struct StubAdapter;
    impl WebhookAdapter for StubAdapter {
        fn platform(&self) -> &str {
            "stub"
        }
        fn verify(&self, _b: &[u8], signature: Option<&str>, _t: Option<&str>) -> bool {
            signature == Some("good")
        }
        fn parse_webhook(&self, _b: &[u8]) -> Result<Vec<MessageEvent>, GatewayError> {
            Ok(vec![MessageEvent {
                platform: "stub".into(),
                chat_id: "c1".into(),
                user_id: "c1".into(),
                text: "hi".into(),
            }])
        }
        fn send_request(&self, m: &OutboundMessage) -> SendRequest {
            // Loopback:1 fails fast — the background deliver is not asserted on.
            SendRequest {
                url: "http://127.0.0.1:1/x".into(),
                auth: SendAuth::None,
                body: SendBody::Json(json!({"t": m.text})),
            }
        }
        fn signature_header(&self) -> Option<&str> {
            Some("x-stub-sig")
        }
        fn verify_get(&self, query: &str) -> Option<String> {
            query.strip_prefix("echo=").map(ToOwned::to_owned)
        }
    }

    /// Like `StubAdapter` but replies synchronously (Teams/Google Chat shape).
    struct SyncStubAdapter;
    impl WebhookAdapter for SyncStubAdapter {
        fn platform(&self) -> &str {
            "sync"
        }
        fn verify(&self, _b: &[u8], signature: Option<&str>, _t: Option<&str>) -> bool {
            signature == Some("good")
        }
        fn parse_webhook(&self, _b: &[u8]) -> Result<Vec<MessageEvent>, GatewayError> {
            Ok(vec![MessageEvent {
                platform: "sync".into(),
                chat_id: "c1".into(),
                user_id: "c1".into(),
                text: "hi".into(),
            }])
        }
        fn send_request(&self, _m: &OutboundMessage) -> SendRequest {
            SendRequest { url: String::new(), auth: SendAuth::None, body: SendBody::Json(json!({})) }
        }
        fn signature_header(&self) -> Option<&str> {
            Some("x-stub-sig")
        }
        fn sync_reply(&self) -> bool {
            true
        }
    }

    struct StubChat;
    #[async_trait]
    impl ChatService for StubChat {
        async fn chat(&self, _s: Option<String>, _m: String) -> Result<ChatReply, DaemonError> {
            Ok(ChatReply { session: "s".into(), reply: "ok".into() })
        }
    }

    fn app() -> Router {
        let mut reg = Registry::new();
        reg.insert("stub".into(), Arc::new(StubAdapter));
        router(reg, Arc::new(StubChat))
    }

    async fn status(sig: Option<&str>, path: &str) -> StatusCode {
        let mut b = Request::post(path);
        if let Some(s) = sig {
            b = b.header("x-stub-sig", s);
        }
        app().oneshot(b.body(axum::body::Body::from("{}")).unwrap()).await.unwrap().status()
    }

    #[tokio::test]
    async fn valid_signature_is_accepted() {
        assert_eq!(status(Some("good"), "/webhook/stub").await, StatusCode::OK);
    }

    #[tokio::test]
    async fn bad_or_missing_signature_is_rejected() {
        assert_eq!(status(Some("bad"), "/webhook/stub").await, StatusCode::UNAUTHORIZED);
        assert_eq!(status(None, "/webhook/stub").await, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn unknown_platform_is_not_found() {
        assert_eq!(status(Some("good"), "/webhook/nope").await, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_handshake_echoes_or_rejects() {
        let app = app();
        // Valid challenge → 200 with the echoed body.
        let resp = app
            .clone()
            .oneshot(Request::get("/webhook/stub?echo=hi").body(axum::body::Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&bytes[..], b"hi");
        // No challenge → 401; unknown platform → 404.
        let reject = app
            .clone()
            .oneshot(Request::get("/webhook/stub").body(axum::body::Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(reject.status(), StatusCode::UNAUTHORIZED);
        let missing = app
            .oneshot(Request::get("/webhook/nope?echo=x").body(axum::body::Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn sync_reply_returns_the_reply_in_the_response_body() {
        let mut reg = Registry::new();
        reg.insert("sync".into(), Arc::new(SyncStubAdapter));
        let app = router(reg, Arc::new(StubChat));
        let req = Request::post("/webhook/sync")
            .header("x-stub-sig", "good")
            .body(axum::body::Body::from("{}"))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        // StubChat replies "ok"; the default sync_response wraps it as {"text": …}.
        assert_eq!(body["text"], "ok");
    }
}
