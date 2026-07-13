//! Unit tests for `slack` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

fn sign(secret: &str, ts: &str, body: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(b"v0:");
    mac.update(ts.as_bytes());
    mac.update(b":");
    mac.update(body);
    format!("v0={}", hex::encode(mac.finalize().into_bytes()))
}

#[test]
fn verifies_fresh_signature_and_rejects_stale_or_wrong() {
    let adapter = SlackAdapter::new("sign-secret", "tok");
    let body = br#"{"type":"event_callback"}"#;
    let now = now_secs().to_string();
    assert!(adapter.verify(body, Some(&sign("sign-secret", &now, body)), Some(&now)));

    // Correct signature but a stale timestamp → rejected by the replay window.
    let old = (now_secs() - 10_000).to_string();
    assert!(!adapter.verify(body, Some(&sign("sign-secret", &old, body)), Some(&old)));

    // Wrong key / missing parts.
    assert!(!adapter.verify(body, Some(&sign("wrong", &now, body)), Some(&now)));
    assert!(!adapter.verify(body, None, Some(&now)));
    assert!(!adapter.verify(body, Some("v0=deadbeef"), None));
}

#[test]
fn parses_user_message_and_skips_bot_and_non_message() {
    let adapter = SlackAdapter::new("s", "t");
    let body = br#"{"type":"event_callback","event":{"type":"message","text":"hi","channel":"C1","user":"U1"}}"#;
    let events = adapter.parse_webhook(body).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].chat_id, "C1");
    assert_eq!(events[0].user_id, "U1");
    assert_eq!(events[0].text, "hi");

    let bot = br#"{"type":"event_callback","event":{"type":"message","text":"x","channel":"C1","bot_id":"B1"}}"#;
    assert!(
        adapter.parse_webhook(bot).unwrap().is_empty(),
        "bot messages are ignored"
    );

    let challenge = br#"{"type":"url_verification","challenge":"abc"}"#;
    assert!(adapter.parse_webhook(challenge).unwrap().is_empty());
}

#[test]
fn send_request_posts_to_chat_postmessage() {
    let adapter = SlackAdapter::new("s", "BOT_TOKEN");
    let req = adapter.send_request(&OutboundMessage {
        chat_id: "C1".into(),
        text: "yo".into(),
    });
    assert_eq!(req.url, POST_MESSAGE_URL);
    assert_eq!(req.auth, SendAuth::Bearer("BOT_TOKEN".into()));
    let SendBody::Json(body) = &req.body else {
        panic!("expected json body")
    };
    assert_eq!(body["channel"], "C1");
    assert_eq!(body["text"], "yo");
}

#[test]
fn complete_upload_body_carries_file_channel_and_optional_comment() {
    let with = slack_complete_body("F123", "C1", "here you go");
    assert_eq!(with["files"][0]["id"], "F123");
    assert_eq!(with["channel_id"], "C1");
    assert_eq!(with["initial_comment"], "here you go");

    // Empty caption → no initial_comment key.
    let without = slack_complete_body("F123", "C1", "");
    assert!(without.get("initial_comment").is_none());
}
