//! Unit tests for `discord_interactions` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use crate::domain::errors::DeaconError;
use crate::infra::http_listener::ChatReply;
use async_trait::async_trait;
use axum::http::Request;
use ed25519_dalek::{Signer, SigningKey};
use tower::ServiceExt;

struct StubChat;
#[async_trait]
impl ChatService for StubChat {
    async fn chat(&self, _s: Option<String>, _m: String) -> Result<ChatReply, DeaconError> {
        Ok(ChatReply {
            session: "s".into(),
            reply: "ok".into(),
        })
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
    assert!(
        !verify(&pk, &sig, "1700000001", body),
        "timestamp is signed"
    );
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
    let app = router(
        pk.to_owned(),
        Arc::new(StubChat),
        Arc::new(AuthPolicy::new(regent_gateway::AuthSnapshot::default())),
        Arc::new(std::env::temp_dir()),
        Arc::new(RateLimiter::per_minute(0)),
    );
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
    assert_eq!(
        post_signed(&sk, &pk, r#"{"type":1}"#, false).await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn bad_signature_is_unauthorized() {
    let (sk, pk) = keypair();
    assert_eq!(
        post_signed(&sk, &pk, r#"{"type":1}"#, true).await,
        StatusCode::UNAUTHORIZED
    );
}
