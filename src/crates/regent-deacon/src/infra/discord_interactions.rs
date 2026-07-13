//! Discord interactions webhook (slash commands) — distinct from the Gateway
//! chat adapter. Discord delivers interactions over HTTP, **Ed25519-signed**,
//! and demands a synchronous response: a `PING` (type 1) must get a `PONG`
//! (type 1), and a command (type 2) must be acked within ~3s. We ack with a
//! *deferred* response (type 5), run the turn in the background, then deliver
//! the reply as a follow-up message (the interaction token authorizes it).

use crate::infra::http_listener::ChatService;
use axum::{
    Json, Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
};
use ed25519_dalek::{Signature, VerifyingKey};
use regent_gateway::{AuthPolicy, RateLimiter};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

const SIG_HEADER: &str = "x-signature-ed25519";
const TS_HEADER: &str = "x-signature-timestamp";

#[derive(Clone)]
struct InteractionsState {
    public_key: Arc<String>,
    service: Arc<dyn ChatService>,
    client: reqwest::Client,
    /// Per-user authorization (default-deny + pairing), shared with the webhook
    /// and gateway planes via `$REGENT_HOME/gateway-auth.json`.
    auth: Arc<AuthPolicy>,
    home: Arc<PathBuf>,
    /// Per-user inbound rate limit (W2.4), shared with the other planes.
    rate: Arc<RateLimiter>,
}

/// Router serving `POST /discord/interactions`, verified against the app's
/// Ed25519 public key (hex).
pub fn router(
    public_key: String,
    service: Arc<dyn ChatService>,
    auth: Arc<AuthPolicy>,
    home: Arc<PathBuf>,
    rate: Arc<RateLimiter>,
) -> Router {
    let state = InteractionsState {
        public_key: Arc::new(public_key),
        service,
        client: reqwest::Client::new(),
        auth,
        home,
        rate,
    };
    Router::new()
        .route("/discord/interactions", post(handle))
        .with_state(state)
}

/// Verifies the Ed25519 signature over `timestamp || body` with the app public
/// key. Any malformed input fails closed.
fn verify(public_key_hex: &str, signature_hex: &str, timestamp: &str, body: &[u8]) -> bool {
    let Ok(pk_bytes) = hex::decode(public_key_hex) else {
        return false;
    };
    let Ok(pk_array) = <[u8; 32]>::try_from(pk_bytes.as_slice()) else {
        return false;
    };
    let Ok(verifying_key) = VerifyingKey::from_bytes(&pk_array) else {
        return false;
    };
    let Ok(sig_bytes) = hex::decode(signature_hex) else {
        return false;
    };
    let Ok(signature) = Signature::from_slice(&sig_bytes) else {
        return false;
    };
    let mut message = timestamp.as_bytes().to_vec();
    message.extend_from_slice(body);
    verifying_key.verify_strict(&message, &signature).is_ok()
}

struct Command {
    text: String,
    channel: String,
    /// Discord user id (guild: `member.user.id`; DM: `user.id`) — the identity
    /// the auth policy evaluates as `discord:<user_id>`.
    user_id: String,
    application_id: String,
    token: String,
}

enum Interaction {
    Ping,
    Command(Command),
    Other,
}

fn parse(body: &[u8]) -> Option<Interaction> {
    let value: Value = serde_json::from_slice(body).ok()?;
    match value.get("type").and_then(Value::as_u64) {
        Some(1) => Some(Interaction::Ping),
        Some(2) => {
            let application_id = value
                .get("application_id")
                .and_then(Value::as_str)?
                .to_owned();
            let token = value.get("token").and_then(Value::as_str)?.to_owned();
            let channel = value
                .get("channel_id")
                .or_else(|| value.pointer("/channel/id"))
                .and_then(Value::as_str)
                .unwrap_or(&application_id)
                .to_owned();
            // Prefer the first string option value (e.g. `/ask question:…`),
            // else the command name itself.
            let user_id = value
                .pointer("/member/user/id")
                .or_else(|| value.pointer("/user/id"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let data = value.get("data");
            let text = data
                .and_then(|d| d.get("options"))
                .and_then(Value::as_array)
                .and_then(|opts| {
                    opts.iter()
                        .find_map(|o| o.get("value").and_then(Value::as_str))
                })
                .or_else(|| data.and_then(|d| d.get("name")).and_then(Value::as_str))?
                .to_owned();
            Some(Interaction::Command(Command {
                text,
                channel,
                user_id,
                application_id,
                token,
            }))
        }
        _ => Some(Interaction::Other),
    }
}

async fn handle(
    State(state): State<InteractionsState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let (Some(signature), Some(timestamp)) = (header(SIG_HEADER), header(TS_HEADER)) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "missing signature"})),
        );
    };
    if !verify(&state.public_key, signature, timestamp, &body) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "bad signature"})),
        );
    }

    match parse(&body) {
        Some(Interaction::Command(cmd)) => {
            // Default-deny per user: an unknown sender's only capability is
            // redeeming a pairing code — reply immediately (type 4), run no turn.
            let user_key = format!("discord:{}", cmd.user_id);
            if !state.auth.is_authorized(&user_key) {
                let msg = if state.auth.try_redeem_code(&cmd.text, &user_key) {
                    let _ =
                        regent_gateway::persist_auth_snapshot(&state.home, &state.auth.snapshot());
                    "✅ Paired! You can talk to the agent now."
                } else {
                    "Not authorized. Ask an operator for a pairing code and send it here."
                };
                return (
                    StatusCode::OK,
                    Json(json!({"type": 4, "data": {"content": msg}})),
                );
            }
            // Rate limit per user (W2.4) — reply immediately (type 4), no turn.
            if !state.rate.check(&user_key) {
                return (
                    StatusCode::OK,
                    Json(json!({"type": 4, "data": {"content":
                        "⏳ You're sending commands too fast — give me a moment."}})),
                );
            }
            // Defer (type 5): ack within Discord's window, deliver as a follow-up.
            let st = state.clone();
            tokio::spawn(async move {
                let key = format!("discord:{}", cmd.channel);
                match st.service.chat_keyed(&key, cmd.text).await {
                    Ok(reply) => {
                        followup(&st.client, &cmd.application_id, &cmd.token, &reply.reply).await
                    }
                    Err(error) => tracing::warn!(%error, "discord interaction turn failed"),
                }
            });
            (StatusCode::OK, Json(json!({"type": 5})))
        }
        // PING (and anything else) → PONG / harmless ack.
        _ => (StatusCode::OK, Json(json!({"type": 1}))),
    }
}

async fn followup(client: &reqwest::Client, application_id: &str, token: &str, content: &str) {
    let url = format!("https://discord.com/api/v10/webhooks/{application_id}/{token}");
    if let Err(error) = client
        .post(&url)
        .json(&json!({"content": content}))
        .send()
        .await
    {
        tracing::warn!(%error, "discord follow-up delivery failed");
    }
}

#[cfg(test)]
#[path = "discord_interactions_tests.rs"]
mod tests;
