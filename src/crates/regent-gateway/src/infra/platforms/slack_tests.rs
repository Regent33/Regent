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

// ── reactions ───────────────────────────────────────────────────────────────

#[test]
fn the_reactions_add_body_names_the_message_by_its_ts() {
    // Slack has no separate message id: `timestamp` IS the identity, and the
    // shortcode must arrive without colons or the call is an invalid_name.
    let body = slack_reaction_body("C123", "1700000000.000100", "🎉");
    assert_eq!(body["channel"], "C123");
    assert_eq!(body["timestamp"], "1700000000.000100");
    assert_eq!(body["name"], "tada");
}

#[test]
fn an_inbound_message_is_remembered_by_channel_and_ts() {
    let adapter = SlackAdapter::new("secret", "xoxb-token");
    let body = br#"{"type":"event_callback","event":
        {"type":"message","channel":"C1","user":"U1","text":"hi","ts":"1700.0001"}}"#;
    assert_eq!(
        adapter.inbound_message_ids(body),
        vec![("C1".to_owned(), "1700.0001".to_owned())]
    );
}

/// Reacting to our own reply would be Regent applauding itself, and an edit is
/// not a new message to acknowledge — both are skipped by `parse_webhook` for
/// the same reason, so both must be skipped here.
#[test]
fn bot_messages_and_edits_are_not_remembered_as_react_targets() {
    let adapter = SlackAdapter::new("secret", "xoxb-token");
    let bot = br#"{"type":"event_callback","event":
        {"type":"message","channel":"C1","bot_id":"B1","text":"mine","ts":"1.1"}}"#;
    let edit = br#"{"type":"event_callback","event":
        {"type":"message","subtype":"message_changed","channel":"C1","text":"x","ts":"1.2"}}"#;
    assert!(adapter.inbound_message_ids(bot).is_empty());
    assert!(adapter.inbound_message_ids(edit).is_empty());
    assert!(adapter.inbound_message_ids(b"not json").is_empty());
}
