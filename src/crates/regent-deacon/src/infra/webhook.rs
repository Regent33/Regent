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

mod delivery;
mod inbound;
mod registry;
mod registry_ext;
pub use delivery::WebhookPlatformDelivery;
use delivery::deliver;
use inbound::handle;
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

#[cfg(test)]
mod tests;
