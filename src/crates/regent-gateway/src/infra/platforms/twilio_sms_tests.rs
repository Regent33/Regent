//! Unit tests for `twilio_sms` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

fn body_of(params: &[(&str, &str)]) -> String {
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for (key, value) in params {
        serializer.append_pair(key, value);
    }
    serializer.finish()
}

#[test]
fn verify_request_accepts_valid_signature_and_rejects_tampering() {
    let adapter = TwilioSmsAdapter::new("AC123", "tok-secret", "+15550000000");
    let url = "https://example.com/webhook/twilio_sms";
    let params = [
        ("Body", "hello"),
        ("From", "+15551234567"),
        ("To", "+15550000000"),
    ];
    let body = body_of(&params);
    let sig = super::super::twilio::sign("tok-secret", url, &params);

    let ok = WebhookRequest {
        url,
        body: body.as_bytes(),
        signature: Some(&sig),
        timestamp: None,
        nonce: None,
    };
    assert!(adapter.verify_request(&ok));

    // Tampered URL / body / wrong-key signature / missing signature.
    let bad_url = WebhookRequest {
        url: "https://evil.test/x",
        body: body.as_bytes(),
        signature: Some(&sig),
        timestamp: None,
        nonce: None,
    };
    assert!(!adapter.verify_request(&bad_url));
    let bad_body = WebhookRequest {
        url,
        body: b"Body=bye&From=x&To=y",
        signature: Some(&sig),
        timestamp: None,
        nonce: None,
    };
    assert!(!adapter.verify_request(&bad_body));
    let wrong_key = super::super::twilio::sign("other", url, &params);
    let bad_key = WebhookRequest {
        url,
        body: body.as_bytes(),
        signature: Some(&wrong_key),
        timestamp: None,
        nonce: None,
    };
    assert!(!adapter.verify_request(&bad_key));
    let no_sig = WebhookRequest {
        url,
        body: body.as_bytes(),
        signature: None,
        timestamp: None,
        nonce: None,
    };
    assert!(!adapter.verify_request(&no_sig));

    // The body-only path always denies (Twilio signs the URL too).
    assert!(!adapter.verify(body.as_bytes(), Some(&sig), None));
}

#[test]
fn parses_sms_and_skips_status_callbacks() {
    let adapter = TwilioSmsAdapter::new("AC1", "t", "+1555");
    let body = body_of(&[
        ("From", "+15551234567"),
        ("Body", "hi there"),
        ("MessageSid", "SM1"),
    ]);
    let events = adapter.parse_webhook(body.as_bytes()).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].chat_id, "+15551234567");
    assert_eq!(events[0].user_id, "+15551234567");
    assert_eq!(events[0].text, "hi there");

    let status = body_of(&[("MessageStatus", "delivered"), ("MessageSid", "SM2")]);
    assert!(adapter.parse_webhook(status.as_bytes()).unwrap().is_empty());
}

#[test]
fn send_request_posts_to_messages_api_with_basic_auth_and_form_body() {
    let adapter = TwilioSmsAdapter::new("AC42", "secret", "+15550000000");
    let req = adapter.send_request(&OutboundMessage {
        chat_id: "+15551234567".into(),
        text: "yo".into(),
    });
    assert_eq!(
        req.url,
        "https://api.twilio.com/2010-04-01/Accounts/AC42/Messages.json"
    );
    assert_eq!(
        req.auth,
        SendAuth::Basic {
            username: "AC42".into(),
            password: "secret".into()
        }
    );
    let SendBody::Form(pairs) = &req.body else {
        panic!("expected form body")
    };
    assert!(pairs.contains(&("From".to_owned(), "+15550000000".to_owned())));
    assert!(pairs.contains(&("To".to_owned(), "+15551234567".to_owned())));
    assert!(pairs.contains(&("Body".to_owned(), "yo".to_owned())));
}
