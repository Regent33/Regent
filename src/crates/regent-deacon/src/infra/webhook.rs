//! Platform webhook ingress (P5). One generic `POST /webhook/{platform}` route
//! serves every `WebhookAdapter`: verify the platform signature → parse events →
//! run a turn → deliver the reply via the platform's API. Adapters are built
//! from environment secrets (loaded from `$REGENT_HOME/.env`); only platforms
//! whose secrets are present are registered.
//!
//! The webhook is acknowledged immediately (a 200) and the turn + reply run in
//! the background, the shape push platforms expect.

use crate::domain::contracts::PlatformDelivery;
use crate::infra::http_listener::ChatService;
use async_trait::async_trait;
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use regent_gateway::{
    AuthPolicy, OutboundMessage, RateLimiter, SendAuth, SendBody, SendRequest, SyncReply,
    WebhookAdapter, WebhookFileSender, WebhookRequest,
};
use regent_kernel::RegentError;
use regent_tools::DeliverySink;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Reply an unauthorized sender gets: a pairing confirmation if this very
/// message redeemed a valid code, else the pairing prompt. Runs no turn.
const PAIRED_MSG: &str = "✅ Paired! You can talk to the agent now.";
const UNAUTHORIZED_MSG: &str =
    "Not authorized. Ask an operator for a pairing code and send it here.";
/// Reply a rate-limited sender gets (W2.4) — no turn runs.
const RATE_LIMITED_MSG: &str = "⏳ You're sending messages too fast — give me a moment.";

mod registry;
use registry::{Registry, delivery_registry_from_env};
pub use registry::{file_senders_from_env, registry_from_env};

#[derive(Clone)]
struct WebhookState {
    registry: Arc<Registry>,
    service: Arc<dyn ChatService>,
    client: reqwest::Client,
    /// Per-user authorization (default-deny + pairing), shared with the gateway
    /// plane via `$REGENT_HOME/gateway-auth.json`.
    auth: Arc<AuthPolicy>,
    home: Arc<PathBuf>,
    /// Per-user inbound rate limit (W2.4), shared with the gateway plane.
    rate: Arc<RateLimiter>,
}

/// Router serving `/webhook/{platform}`: `POST` for events, `GET` for the
/// echostr endpoint-verification handshake (WeChat/WeCom).
pub fn router(
    registry: Registry,
    service: Arc<dyn ChatService>,
    auth: Arc<AuthPolicy>,
    home: Arc<PathBuf>,
    rate: Arc<RateLimiter>,
) -> Router {
    let state = WebhookState {
        registry: Arc::new(registry),
        service,
        client: reqwest::Client::new(),
        auth,
        home,
        rate,
    };
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

/// Per-user authorization gate. Returns the reply to send back when the sender
/// is NOT allowed to run a turn — a pairing confirmation if `text` was a valid
/// code (persisted so it survives restart), else the pairing prompt. `None`
/// means authorized → run the turn. Default-deny: an unknown user's only
/// capability is redeeming a pairing code.
fn gate(state: &WebhookState, platform: &str, user_id: &str, text: &str) -> Option<&'static str> {
    let user_key = format!("{platform}:{user_id}");
    if state.auth.is_authorized(&user_key) {
        return None;
    }
    if state.auth.try_redeem_code(text, &user_key) {
        let _ = regent_gateway::persist_auth_snapshot(&state.home, &state.auth.snapshot());
        return Some(PAIRED_MSG);
    }
    Some(UNAUTHORIZED_MSG)
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
        name.and_then(|n| headers.get(n))
            .and_then(|v| v.to_str().ok())
            .map(ToOwned::to_owned)
    };
    let signature = header(adapter.signature_header());
    let timestamp = header(adapter.timestamp_header());
    let nonce = header(adapter.nonce_header());

    // Reconstruct the full public URL (HTTP/1.1 request targets are origin-form,
    // so scheme/host live in proxy headers). Only URL-signing schemes (Twilio)
    // read it; body-only adapters ignore it via the default `verify_request`.
    let scheme = header(Some("x-forwarded-proto")).unwrap_or_else(|| "https".to_owned());
    let host = header(Some("x-forwarded-host"))
        .or_else(|| header(Some("host")))
        .unwrap_or_default();
    let path_and_query = uri
        .path_and_query()
        .map_or_else(|| uri.path(), |pq| pq.as_str());
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
        // Rate-limit before authz so an unauthorized flooder is throttled too
        // (else every spam message would still cost an outbound "not authorized"
        // reply). Each user_key has its own bucket, so this only throttles the
        // sender's own flood.
        if !state.rate.check(&format!("{platform}:{}", event.user_id)) {
            return render_sync(adapter.sync_response(RATE_LIMITED_MSG));
        }
        if let Some(reply) = gate(&state, &platform, &event.user_id, &event.text) {
            return render_sync(adapter.sync_response(reply));
        }
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
            // Rate-limit before authz — an unauthorized flooder is throttled too.
            if !state.rate.check(&format!("{platform}:{}", event.user_id)) {
                let out = OutboundMessage {
                    chat_id: event.chat_id,
                    text: RATE_LIMITED_MSG.to_owned(),
                };
                deliver(&state.client, &adapter.send_request(&out)).await;
                continue;
            }
            if let Some(reply) = gate(&state, &platform, &event.user_id, &event.text) {
                let out = OutboundMessage {
                    chat_id: event.chat_id,
                    text: reply.to_owned(),
                };
                deliver(&state.client, &adapter.send_request(&out)).await;
                continue;
            }
            // One continuous session per platform conversation.
            let key = format!("{platform}:{}", event.chat_id);
            let reply = match state.service.chat_keyed(&key, event.text).await {
                Ok(reply) => reply.reply,
                Err(error) => {
                    tracing::warn!(%error, platform, "webhook turn failed");
                    continue;
                }
            };
            let out = OutboundMessage {
                chat_id: event.chat_id,
                text: reply,
            };
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

/// Routes a keyed platform session's `send_message`/`send_file` back to the
/// platform's API. Built from env (adapters are stateless, so reconstructing
/// them here rather than sharing the router's registry is cheap and keeps the
/// router signature untouched).
pub struct WebhookPlatformDelivery {
    adapters: Registry,
    file_senders: HashMap<String, Arc<dyn WebhookFileSender>>,
    client: reqwest::Client,
}

impl WebhookPlatformDelivery {
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            // Outbound only — no inbound verification here, so use the registry
            // variant that doesn't spawn a duplicate Google Chat JWKS refresher.
            adapters: delivery_registry_from_env(),
            file_senders: file_senders_from_env(),
            client: reqwest::Client::new(),
        }
    }
}

impl PlatformDelivery for WebhookPlatformDelivery {
    fn sink_for(&self, conversation_key: &str) -> Option<Arc<dyn DeliverySink>> {
        let (platform, chat_id) = conversation_key.split_once(':')?;
        let adapter = self.adapters.get(platform)?;
        Some(Arc::new(WebhookDelivery {
            platform: platform.to_owned(),
            chat_id: chat_id.to_owned(),
            adapter: Arc::clone(adapter),
            file_sender: self.file_senders.get(platform).cloned(),
            client: self.client.clone(),
        }))
    }
}

/// One platform conversation's outbound sink: text via the adapter's
/// `send_request`, files via its [`WebhookFileSender`] (when it has one).
struct WebhookDelivery {
    platform: String,
    chat_id: String,
    adapter: Arc<dyn WebhookAdapter>,
    file_sender: Option<Arc<dyn WebhookFileSender>>,
    client: reqwest::Client,
}

#[async_trait]
impl DeliverySink for WebhookDelivery {
    async fn deliver(&self, _target: &str, text: &str) -> Result<(), RegentError> {
        let message = OutboundMessage {
            chat_id: self.chat_id.clone(),
            text: text.to_owned(),
        };
        deliver(&self.client, &self.adapter.send_request(&message)).await;
        Ok(())
    }

    fn targets(&self) -> Vec<String> {
        vec![format!("{}:{}", self.platform, self.chat_id)]
    }

    async fn deliver_file(
        &self,
        _target: &str,
        path: &std::path::Path,
        caption: &str,
    ) -> Result<(), RegentError> {
        match &self.file_sender {
            Some(sender) => sender
                .send_file(&self.client, &self.chat_id, path, caption)
                .await
                .map_err(|e| RegentError::Tool {
                    tool: "send_file".into(),
                    message: e.to_string(),
                }),
            None => Err(RegentError::Tool {
                tool: "send_file".into(),
                message: format!("file upload is not supported on {}", self.platform),
            }),
        }
    }
}

#[cfg(test)]
mod tests;
