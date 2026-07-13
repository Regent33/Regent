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
        Self {
            auth_token: auth_token.into(),
            greeting: greeting.into(),
        }
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
#[path = "twilio_voice_tests.rs"]
mod tests;
