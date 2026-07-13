//! Unit tests for `feishu` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

fn req<'a>(body: &'a [u8], sig: Option<&'a str>, nonce: Option<&'a str>) -> WebhookRequest<'a> {
    WebhookRequest {
        url: "",
        body,
        signature: sig,
        timestamp: Some("1700000000"),
        nonce,
    }
}

#[test]
fn plaintext_mode_verifies_token_and_handshakes() {
    let adapter = FeishuAdapter::new("vtok", None, None);
    let body = br#"{"type":"url_verification","challenge":"C9","token":"vtok"}"#;
    assert!(adapter.verify_request(&req(body, None, None)));
    let SyncReply::Json(reply) = adapter.handshake(body).unwrap() else {
        panic!("json")
    };
    assert_eq!(reply["challenge"], "C9");

    // Wrong token → rejected.
    let bad = br#"{"type":"url_verification","challenge":"C9","token":"nope"}"#;
    assert!(!adapter.verify_request(&req(bad, None, None)));
}

#[test]
fn encrypted_mode_verifies_signature_and_decrypts() {
    let adapter = FeishuAdapter::new("vtok", Some("ekey".to_owned()), None);
    let plain = br#"{"type":"url_verification","challenge":"XYZ"}"#;
    let blob = feishu_crypto::encrypt("ekey", plain, &[3u8; 16]);
    let body = format!(r#"{{"encrypt":"{blob}"}}"#);
    let sig = feishu_crypto::sign("1700000000", "n1", "ekey", body.as_bytes());

    assert!(adapter.verify_request(&req(body.as_bytes(), Some(&sig), Some("n1"))));
    // Tampered signature and missing nonce are rejected.
    assert!(!adapter.verify_request(&req(body.as_bytes(), Some("deadbeef"), Some("n1"))));
    assert!(!adapter.verify_request(&req(body.as_bytes(), Some(&sig), None)));

    let SyncReply::Json(reply) = adapter.handshake(body.as_bytes()).unwrap() else {
        panic!()
    };
    assert_eq!(reply["challenge"], "XYZ");
}

#[test]
fn parses_message_event_and_skips_others() {
    let adapter = FeishuAdapter::new("vtok", None, None);
    let body = br#"{"schema":"2.0",
        "header":{"event_type":"im.message.receive_v1","token":"vtok"},
        "event":{"message":{"chat_id":"oc_1","content":"{\"text\":\"hi there\"}"},
                 "sender":{"sender_id":{"open_id":"ou_1"}}}}"#;
    let events = adapter.parse_webhook(body).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].chat_id, "oc_1");
    assert_eq!(events[0].user_id, "ou_1");
    assert_eq!(events[0].text, "hi there");

    let other = br#"{"schema":"2.0","header":{"event_type":"im.chat.updated_v1"},"event":{}}"#;
    assert!(adapter.parse_webhook(other).unwrap().is_empty());
}

#[test]
fn send_request_targets_the_messages_api() {
    let adapter = FeishuAdapter::new("vtok", None, Some("tk".to_owned()));
    let req = adapter.send_request(&OutboundMessage {
        chat_id: "oc_1".into(),
        text: "yo".into(),
    });
    assert!(req.url.contains("/im/v1/messages"));
    assert_eq!(req.auth, SendAuth::Bearer("tk".into()));
    let SendBody::Json(body) = &req.body else {
        panic!("json body")
    };
    assert_eq!(body["receive_id"], "oc_1");
    assert_eq!(body["msg_type"], "text");
    assert!(body["content"].as_str().unwrap().contains("yo"));
}
