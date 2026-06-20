//! Twilio **Voice** adapter — a speech IVR over the same webhook shape as SMS.
//! Twilio POSTs `application/x-www-form-urlencoded` call events signed with
//! `X-Twilio-Signature` (URL + params; see [`super::twilio`]). The reply is
//! **TwiML** (XML) returned synchronously: we `<Say>` the agent's text and then
//! `<Gather input="speech">` to capture the caller's next utterance — Twilio's
//! built-in speech recognition transcribes it and POSTs `SpeechResult` back to
//! the same URL, so the whole call is one session keyed by `CallSid`. No
//! external STT/TTS service is needed.

use crate::domain::contracts::{
    SendAuth, SendBody, SendRequest, SyncReply, WebhookAdapter, WebhookRequest,
};
use crate::domain::entities::{MessageEvent, OutboundMessage};
use crate::domain::errors::GatewayError;

pub struct TwilioVoiceAdapter {
    auth_token: String,
    /// Spoken on the initial call, before any speech is captured.
    greeting: String,
}

impl TwilioVoiceAdapter {
    #[must_use]
    pub fn new(auth_token: impl Into<String>, greeting: impl Into<String>) -> Self {
        Self { auth_token: auth_token.into(), greeting: greeting.into() }
    }
}

/// Escapes the five XML entities so an agent reply is safe inside `<Say>`.
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// `<Say>` the text, then `<Gather>` the caller's next speech (looping back to
/// the same URL, the `<Gather>` default action).
fn twiml(say: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Response><Say>{}</Say>\
         <Gather input=\"speech\" speechTimeout=\"auto\"/></Response>",
        xml_escape(say)
    )
}

impl WebhookAdapter for TwilioVoiceAdapter {
    fn platform(&self) -> &str {
        "twilio_voice"
    }

    /// Twilio signs the URL + params, not the body alone — deny the body-only
    /// path (the route uses `verify_request`).
    fn verify(&self, _body: &[u8], _signature: Option<&str>, _timestamp: Option<&str>) -> bool {
        false
    }

    fn verify_request(&self, request: &WebhookRequest<'_>) -> bool {
        super::twilio::verify_signature(&self.auth_token, request)
    }

    fn parse_webhook(&self, body: &[u8]) -> Result<Vec<MessageEvent>, GatewayError> {
        let mut call_sid = None;
        let mut speech = None;
        for (key, value) in form_urlencoded::parse(body) {
            match key.as_ref() {
                "CallSid" => call_sid = Some(value.into_owned()),
                "SpeechResult" => speech = Some(value.into_owned()),
                _ => {}
            }
        }
        // No transcription yet (initial call / non-speech callback): the route
        // falls back to `sync_idle_response` (the greeting + a `<Gather>`).
        let (Some(call_sid), Some(text)) = (call_sid, speech) else {
            return Ok(Vec::new());
        };
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }
        Ok(vec![MessageEvent {
            platform: "twilio_voice".to_owned(),
            chat_id: call_sid.clone(),
            user_id: call_sid,
            text,
        }])
    }

    fn send_request(&self, _message: &OutboundMessage) -> SendRequest {
        // Voice replies synchronously as TwiML (see `sync_reply`); the route
        // never calls this for this adapter.
        SendRequest {
            url: String::new(),
            auth: SendAuth::None,
            body: SendBody::Json(serde_json::Value::Null),
        }
    }

    fn signature_header(&self) -> Option<&str> {
        Some("x-twilio-signature")
    }

    fn sync_reply(&self) -> bool {
        true
    }

    fn sync_response(&self, reply: &str) -> SyncReply {
        SyncReply::Xml(twiml(reply))
    }

    fn sync_idle_response(&self) -> Option<SyncReply> {
        Some(SyncReply::Xml(twiml(&self.greeting)))
    }
}

#[cfg(test)]
mod tests {
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

        let ok = WebhookRequest { url, body: body.as_bytes(), signature: Some(&sig), timestamp: None, nonce: None };
        assert!(adapter.verify_request(&ok));
        let bad =
            WebhookRequest { url: "https://evil.test/x", body: body.as_bytes(), signature: Some(&sig), timestamp: None, nonce: None };
        assert!(!adapter.verify_request(&bad));
        let no_sig = WebhookRequest { url, body: body.as_bytes(), signature: None, timestamp: None, nonce: None };
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
        assert!(adapter.parse_webhook(initial.as_bytes()).unwrap().is_empty());
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
}
