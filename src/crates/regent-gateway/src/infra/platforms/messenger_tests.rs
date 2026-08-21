//! Unit tests for `messenger` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

fn sign(secret: &str, body: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

#[test]
fn verifies_a_valid_signature_and_rejects_others() {
    let adapter = MessengerAdapter::new("app-secret", "tok");
    let body = br#"{"object":"page"}"#;
    assert!(adapter.verify(body, Some(&sign("app-secret", body)), None));
    assert!(
        !adapter.verify(body, Some("sha256=deadbeef"), None),
        "wrong digest"
    );
    assert!(
        !adapter.verify(body, None, None),
        "missing signature → deny"
    );
    assert!(
        !adapter.verify(body, Some(&sign("other-secret", body)), None),
        "wrong key"
    );
}

#[test]
fn parses_text_messaging_events() {
    let adapter = MessengerAdapter::new("s", "t");
    let body = br#"{"object":"page","entry":[{"messaging":[
        {"sender":{"id":"USER123"},"message":{"text":"hello"}},
        {"sender":{"id":"USER123"},"delivery":{}}
    ]}]}"#;
    let events = adapter.parse_webhook(body).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].chat_id, "USER123");
    assert_eq!(events[0].text, "hello");
    assert_eq!(events[0].chat_key(), "messenger:USER123");
}

#[test]
fn send_request_targets_the_graph_send_api() {
    let adapter = MessengerAdapter::new("s", "PAGE_TOKEN");
    let req = adapter.send_request(&OutboundMessage {
        chat_id: "U1".into(),
        text: "hi".into(),
    });
    assert_eq!(req.url, GRAPH_SEND_URL);
    assert_eq!(req.auth, SendAuth::Bearer("PAGE_TOKEN".into()));
    let SendBody::Json(body) = &req.body else {
        panic!("expected json body")
    };
    assert_eq!(body["recipient"]["id"], "U1");
    assert_eq!(body["message"]["text"], "hi");
}

// ── reactions ───────────────────────────────────────────────────────────────

#[test]
fn the_reaction_body_is_a_sender_action_with_a_fixed_set_value() {
    // The Send API rejects anything outside its seven reactions, so 🤯 has to
    // arrive as `wow` rather than as itself.
    let body = messenger_reaction_body("PSID1", "mid.ABC", "🤯");
    assert_eq!(body["recipient"]["id"], "PSID1");
    assert_eq!(body["sender_action"], "react");
    assert_eq!(body["payload"]["message_id"], "mid.ABC");
    assert_eq!(body["payload"]["reaction"], "wow");
}

#[test]
fn inbound_mids_are_paired_with_the_sender() {
    let adapter = MessengerAdapter::new("secret", "page-token");
    let body = br#"{"entry":[{"messaging":[
        {"sender":{"id":"PSID1"},"message":{"mid":"mid.ONE","text":"hi"}}]}]}"#;
    assert_eq!(
        adapter.inbound_message_ids(body),
        vec![("PSID1".to_owned(), "mid.ONE".to_owned())]
    );
}

/// Delivery and read receipts share the webhook and have no `message.mid`.
#[test]
fn receipts_and_junk_yield_no_react_targets() {
    let adapter = MessengerAdapter::new("secret", "page-token");
    let receipt = br#"{"entry":[{"messaging":[
        {"sender":{"id":"PSID1"},"delivery":{"watermark":1}}]}]}"#;
    assert!(adapter.inbound_message_ids(receipt).is_empty());
    assert!(adapter.inbound_message_ids(b"not json").is_empty());
}
