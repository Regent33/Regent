//! Unit tests for `twilio_voice` (extracted for the file-size rule; same
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
    let adapter = TwilioVoiceAdapter::new("tok-secret", "Hi");
    let url = "https://example.com/webhook/twilio_voice";
    let params = [("CallSid", "CA1"), ("SpeechResult", "hello agent")];
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
    let bad = WebhookRequest {
        url: "https://evil.test/x",
        body: body.as_bytes(),
        signature: Some(&sig),
        timestamp: None,
        nonce: None,
    };
    assert!(!adapter.verify_request(&bad));
    let no_sig = WebhookRequest {
        url,
        body: body.as_bytes(),
        signature: None,
        timestamp: None,
        nonce: None,
    };
    assert!(!adapter.verify_request(&no_sig));
}

#[test]
fn parses_speech_and_skips_the_initial_call() {
    let adapter = TwilioVoiceAdapter::new("t", "Hi");
    let with_speech = body_of(&[("CallSid", "CA9"), ("SpeechResult", "what is the weather")]);
    let events = adapter.parse_webhook(with_speech.as_bytes()).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].chat_id, "CA9");
    assert_eq!(events[0].text, "what is the weather");

    // Initial call (no SpeechResult) and empty transcription → no event.
    let initial = body_of(&[("CallSid", "CA9"), ("From", "+1555")]);
    assert!(
        adapter
            .parse_webhook(initial.as_bytes())
            .unwrap()
            .is_empty()
    );
    let empty = body_of(&[("CallSid", "CA9"), ("SpeechResult", "   ")]);
    assert!(adapter.parse_webhook(empty.as_bytes()).unwrap().is_empty());
}

#[test]
fn renders_twiml_for_greeting_and_reply_with_escaping() {
    let adapter = TwilioVoiceAdapter::new("t", "Welcome to Regent");
    assert!(adapter.sync_reply());

    let SyncReply::Xml(idle) = adapter.sync_idle_response().unwrap() else {
        panic!("expected xml");
    };
    assert!(idle.contains("<Say>Welcome to Regent</Say>"));
    assert!(idle.contains("<Gather input=\"speech\""));

    let SyncReply::Xml(reply) = adapter.sync_response("Tom & Jerry <ok>") else {
        panic!("expected xml");
    };
    assert!(reply.contains("Tom &amp; Jerry &lt;ok&gt;"));
    assert!(reply.contains("<Gather"));
}
