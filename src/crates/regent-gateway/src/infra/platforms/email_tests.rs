//! Unit tests for `email` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

fn body_of(params: &[(&str, &str)]) -> String {
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for (key, value) in params {
        serializer.append_pair(key, value);
    }
    serializer.finish()
}

/// Mailgun's signature: hex(HMAC-SHA256(signing_key, timestamp + token)).
fn sign(signing_key: &str, timestamp: &str, token: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(signing_key.as_bytes()).unwrap();
    mac.update(format!("{timestamp}{token}").as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

#[test]
fn verifies_a_valid_body_signature_and_rejects_others() {
    let adapter = EmailAdapter::new(
        "sign-key",
        "key-api",
        "mg.example.com",
        "bot@mg.example.com",
    );
    let (ts, token) = ("1700000000", "abc-token");
    let sig = sign("sign-key", ts, token);
    let body = body_of(&[("timestamp", ts), ("token", token), ("signature", &sig)]);
    assert!(adapter.verify(body.as_bytes(), None, None));

    // Tampered signature.
    let tampered = body_of(&[
        ("timestamp", ts),
        ("token", token),
        ("signature", "deadbeef"),
    ]);
    assert!(
        !adapter.verify(tampered.as_bytes(), None, None),
        "wrong digest → deny"
    );

    // Wrong signing key.
    let wrong_key = sign("other-key", ts, token);
    let bad_key = body_of(&[
        ("timestamp", ts),
        ("token", token),
        ("signature", &wrong_key),
    ]);
    assert!(
        !adapter.verify(bad_key.as_bytes(), None, None),
        "wrong key → deny"
    );

    // Missing fields → deny.
    let missing = body_of(&[("timestamp", ts), ("token", token)]);
    assert!(
        !adapter.verify(missing.as_bytes(), None, None),
        "missing signature → deny"
    );
    assert!(!adapter.verify(b"", None, None), "empty body → deny");
}

#[test]
fn parses_sender_and_body_plain_into_an_event() {
    let adapter = EmailAdapter::new("s", "k", "mg.example.com", "bot@mg.example.com");
    let body = body_of(&[
        ("sender", "alice@example.com"),
        ("subject", "Hello"),
        ("body-plain", "please help"),
    ]);
    let events = adapter.parse_webhook(body.as_bytes()).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].chat_id, "alice@example.com");
    assert_eq!(events[0].user_id, "alice@example.com");
    assert_eq!(events[0].text, "please help");
    assert_eq!(events[0].chat_key(), "email:alice@example.com");
}

#[test]
fn falls_back_to_subject_when_body_plain_is_empty() {
    let adapter = EmailAdapter::new("s", "k", "d", "f");
    let body = body_of(&[
        ("sender", "alice@example.com"),
        ("subject", "the subject"),
        ("body-plain", ""),
    ]);
    let events = adapter.parse_webhook(body.as_bytes()).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].text, "the subject");
}

#[test]
fn skips_a_body_with_no_sender() {
    let adapter = EmailAdapter::new("s", "k", "d", "f");
    let body = body_of(&[("subject", "no sender"), ("body-plain", "orphan")]);
    assert!(adapter.parse_webhook(body.as_bytes()).unwrap().is_empty());
}

#[test]
fn send_request_posts_to_messages_api_with_basic_auth_and_form_body() {
    let adapter = EmailAdapter::new("s", "key-secret", "mg.example.com", "bot@mg.example.com");
    let req = adapter.send_request(&OutboundMessage {
        chat_id: "alice@example.com".into(),
        text: "hi back".into(),
    });
    assert_eq!(
        req.url,
        "https://api.mailgun.net/v3/mg.example.com/messages"
    );
    assert_eq!(
        req.auth,
        SendAuth::Basic {
            username: "api".into(),
            password: "key-secret".into()
        }
    );
    let SendBody::Form(pairs) = &req.body else {
        panic!("expected form body")
    };
    assert!(pairs.contains(&("from".to_owned(), "bot@mg.example.com".to_owned())));
    assert!(pairs.contains(&("to".to_owned(), "alice@example.com".to_owned())));
    assert!(pairs.contains(&("text".to_owned(), "hi back".to_owned())));
    assert!(pairs.contains(&("subject".to_owned(), "Re: your message".to_owned())));
}

#[test]
fn signature_lives_in_the_body_not_a_header() {
    let adapter = EmailAdapter::new("s", "k", "d", "f");
    assert_eq!(adapter.signature_header(), None);
    assert_eq!(adapter.platform(), "email");
}
