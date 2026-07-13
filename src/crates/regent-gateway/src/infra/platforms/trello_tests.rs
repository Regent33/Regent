//! Unit tests for `trello` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

fn sign(secret: &str, body: &[u8], url: &str) -> String {
    let mut mac = HmacSha1::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    mac.update(url.as_bytes());
    STANDARD.encode(mac.finalize().into_bytes())
}

#[test]
fn verify_request_accepts_valid_signature_and_rejects_tampering() {
    let adapter = TrelloAdapter::new("app-secret", "k", "t");
    let url = "https://example.com/webhook/trello";
    let body = br#"{"action":{"type":"commentCard"}}"#;
    let sig = sign("app-secret", body, url);

    let ok = WebhookRequest {
        url,
        body,
        signature: Some(&sig),
        timestamp: None,
        nonce: None,
    };
    assert!(adapter.verify_request(&ok));

    // Tampered callback URL, body, wrong key, and missing signature all fail.
    let bad_url = WebhookRequest {
        url: "https://evil.test/x",
        body,
        signature: Some(&sig),
        timestamp: None,
        nonce: None,
    };
    assert!(!adapter.verify_request(&bad_url));
    let bad_body = WebhookRequest {
        url,
        body: br#"{"x":1}"#,
        signature: Some(&sig),
        timestamp: None,
        nonce: None,
    };
    assert!(!adapter.verify_request(&bad_body));
    let wrong = sign("other-secret", body, url);
    let bad_key = WebhookRequest {
        url,
        body,
        signature: Some(&wrong),
        timestamp: None,
        nonce: None,
    };
    assert!(!adapter.verify_request(&bad_key));
    let no_sig = WebhookRequest {
        url,
        body,
        signature: None,
        timestamp: None,
        nonce: None,
    };
    assert!(!adapter.verify_request(&no_sig));
}

#[test]
fn get_handshake_returns_ok_for_liveness_check() {
    let adapter = TrelloAdapter::new("s", "k", "t");
    assert_eq!(adapter.verify_get(""), Some(String::new()));
}

#[test]
fn parses_comment_card_and_skips_other_actions() {
    let adapter = TrelloAdapter::new("s", "k", "t");
    let body = br#"{"action":{"type":"commentCard",
        "data":{"text":"please review","card":{"id":"card_9","name":"Ship it"}},
        "memberCreator":{"id":"mem_1","username":"sam"}}}"#;
    let events = adapter.parse_webhook(body).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].chat_id, "card_9");
    assert_eq!(events[0].user_id, "mem_1");
    assert_eq!(events[0].text, "please review");

    let other = br#"{"action":{"type":"updateCard","data":{"card":{"id":"c1"}}}}"#;
    assert!(adapter.parse_webhook(other).unwrap().is_empty());
}

#[test]
fn send_request_posts_a_card_comment() {
    let adapter = TrelloAdapter::new("s", "API-KEY", "TOKEN");
    let req = adapter.send_request(&OutboundMessage {
        chat_id: "card_9".into(),
        text: "on it".into(),
    });
    assert_eq!(
        req.url,
        "https://api.trello.com/1/cards/card_9/actions/comments"
    );
    assert_eq!(req.auth, SendAuth::None);
    let SendBody::Form(pairs) = &req.body else {
        panic!("expected form body")
    };
    assert!(pairs.contains(&("key".to_owned(), "API-KEY".to_owned())));
    assert!(pairs.contains(&("token".to_owned(), "TOKEN".to_owned())));
    assert!(pairs.contains(&("text".to_owned(), "on it".to_owned())));
}
