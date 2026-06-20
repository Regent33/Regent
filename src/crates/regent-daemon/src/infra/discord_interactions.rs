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
use serde_json::{Value, json};
use std::sync::Arc;

const SIG_HEADER: &str = "x-signature-ed25519";
const TS_HEADER: &str = "x-signature-timestamp";

#[derive(Clone)]
struct InteractionsState {
    public_key: Arc<String>,
    service: Arc<dyn ChatService>,
    client: reqwest::Client,
}

/// Router serving `POST /discord/interactions`, verified against the app's
/// Ed25519 public key (hex).
pub fn router(public_key: String, service: Arc<dyn ChatService>) -> Router {
    let state =
        InteractionsState { public_key: Arc::new(public_key), service, client: reqwest::Client::new() };
    Router::new().route("/discord/interactions", post(handle)).with_state(state)
}

/// Verifies the Ed25519 signature over `timestamp || body` with the app public
/// key. Any malformed input fails closed.
fn verify(public_key_hex: &str, signature_hex: &str, timestamp: &str, body: &[u8]) -> bool {
    let Ok(pk_bytes) = hex::decode(public_key_hex) else { return false };
    let Ok(pk_array) = <[u8; 32]>::try_from(pk_bytes.as_slice()) else { return false };
    let Ok(verifying_key) = VerifyingKey::from_bytes(&pk_array) else { return false };
    let Ok(sig_bytes) = hex::decode(signature_hex) else { return false };
    let Ok(signature) = Signature::from_slice(&sig_bytes) else { return false };
    let mut message = timestamp.as_bytes().to_vec();
    message.extend_from_slice(body);
    verifying_key.verify_strict(&message, &signature).is_ok()
}

struct Command {
    text: String,
    channel: String,
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
            let application_id = value.get("application_id").and_then(Value::as_str)?.to_owned();
            let token = value.get("token").and_then(Value::as_str)?.to_owned();
            let channel = value
                .get("channel_id")
                .or_else(|| value.pointer("/channel/id"))
                .and_then(Value::as_str)
                .unwrap_or(&application_id)
                .to_owned();
            // Prefer the first string option value (e.g. `/ask question:…`),
            // else the command name itself.
            let data = value.get("data");
            let text = data
                .and_then(|d| d.get("options"))
                .and_then(Value::as_array)
                .and_then(|opts| opts.iter().find_map(|o| o.get("value").and_then(Value::as_str)))
                .or_else(|| data.and_then(|d| d.get("name")).and_then(Value::as_str))?
                .to_owned();
            Some(Interaction::Command(Command { text, channel, application_id, token }))
        }
        _ => Some(Interaction::Other),
    }
}

async fn handle(State(state): State<InteractionsState>, headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let (Some(signature), Some(timestamp)) = (header(SIG_HEADER), header(TS_HEADER)) else {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "missing signature"})));
    };
    if !verify(&state.public_key, signature, timestamp, &body) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "bad signature"})));
    }

    match parse(&body) {
        Some(Interaction::Command(cmd)) => {
            // Defer (type 5): ack within Discord's window, deliver as a follow-up.
            let st = state.clone();
            tokio::spawn(async move {
                let key = format!("discord:{}", cmd.channel);
                match st.service.chat_keyed(&key, cmd.text).await {
                    Ok(reply) => followup(&st.client, &cmd.application_id, &cmd.token, &reply.reply).await,
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
    if let Err(error) = client.post(&url).json(&json!({"content": content})).send().await {
        tracing::warn!(%error, "discord follow-up delivery failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use crate::domain::errors::DaemonError;
    use crate::infra::http_listener::ChatReply;
    use async_trait::async_trait;
    use ed25519_dalek::{Signer, SigningKey};
    use tower::ServiceExt;

    struct StubChat;
    #[async_trait]
    impl ChatService for StubChat {
        async fn chat(&self, _s: Option<String>, _m: String) -> Result<ChatReply, DaemonError> {
            Ok(ChatReply { session: "s".into(), reply: "ok".into() })
        }
    }

    fn keypair() -> (SigningKey, String) {
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let pk_hex = hex::encode(sk.verifying_key().to_bytes());
        (sk, pk_hex)
    }

    #[test]
    fn verifies_valid_signature_and_rejects_tampering() {
        let (sk, pk) = keypair();
        let (ts, body) = ("1700000000", br#"{"type":1}"#.as_slice());
        let mut msg = ts.as_bytes().to_vec();
        msg.extend_from_slice(body);
        let sig = hex::encode(sk.sign(&msg).to_bytes());

        assert!(verify(&pk, &sig, ts, body));
        assert!(!verify(&pk, &sig, "1700000001", body), "timestamp is signed");
        assert!(!verify(&pk, &sig, ts, br#"{"type":2}"#), "body is signed");
        assert!(!verify(&pk, "zz", ts, body), "garbage signature");
    }

    #[test]
    fn parses_ping_and_command() {
        assert!(matches!(parse(br#"{"type":1}"#), Some(Interaction::Ping)));
        let cmd = br#"{"type":2,"application_id":"app1","token":"tok1","channel_id":"C9",
            "data":{"name":"ask","options":[{"name":"q","value":"hello there"}]}}"#;
        match parse(cmd) {
            Some(Interaction::Command(c)) => {
                assert_eq!(c.text, "hello there");
                assert_eq!(c.channel, "C9");
                assert_eq!(c.application_id, "app1");
                assert_eq!(c.token, "tok1");
            }
            _ => panic!("expected a command"),
        }
    }

    async fn post_signed(sk: &SigningKey, pk: &str, body: &'static str, tamper: bool) -> StatusCode {
        let ts = "1700000000";
        let mut msg = ts.as_bytes().to_vec();
        msg.extend_from_slice(body.as_bytes());
        let sig = hex::encode(sk.sign(&msg).to_bytes());
        let app = router(pk.to_owned(), Arc::new(StubChat));
        let req = Request::post("/discord/interactions")
            .header(SIG_HEADER, if tamper { "00".to_owned() } else { sig })
            .header(TS_HEADER, ts)
            .body(axum::body::Body::from(body))
            .unwrap();
        app.oneshot(req).await.unwrap().status()
    }

    #[tokio::test]
    async fn ping_with_valid_signature_returns_ok() {
        let (sk, pk) = keypair();
        assert_eq!(post_signed(&sk, &pk, r#"{"type":1}"#, false).await, StatusCode::OK);
    }

    #[tokio::test]
    async fn bad_signature_is_unauthorized() {
        let (sk, pk) = keypair();
        assert_eq!(post_signed(&sk, &pk, r#"{"type":1}"#, true).await, StatusCode::UNAUTHORIZED);
    }
}
