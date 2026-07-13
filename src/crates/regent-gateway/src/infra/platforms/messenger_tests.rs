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
