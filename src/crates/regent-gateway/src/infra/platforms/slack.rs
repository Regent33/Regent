//! Slack Events API webhook adapter. Slack signs the base string
//! `v0:{timestamp}:{body}` with the app signing secret (HMAC-SHA256, hex),
//! delivered as `X-Slack-Signature` with the timestamp in
//! `X-Slack-Request-Timestamp`. Verification also rejects stale timestamps (a
//! replay window). Replies go out via chat.postMessage. Parse/build are pure;
//! verify touches the wall clock only for the replay check.

use crate::domain::contracts::{SendAuth, SendBody, SendRequest, WebhookAdapter};
use crate::domain::entities::{MessageEvent, OutboundMessage};
use crate::domain::errors::GatewayError;
use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

const POST_MESSAGE_URL: &str = "https://slack.com/api/chat.postMessage";
/// Slack's recommended replay window.
const MAX_SKEW_SECS: i64 = 60 * 5;

pub struct SlackAdapter {
    signing_secret: String,
    bot_token: String,
}

impl SlackAdapter {
    #[must_use]
    pub fn new(signing_secret: impl Into<String>, bot_token: impl Into<String>) -> Self {
        Self { signing_secret: signing_secret.into(), bot_token: bot_token.into() }
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

impl WebhookAdapter for SlackAdapter {
    fn platform(&self) -> &str {
        "slack"
    }

    fn verify(&self, body: &[u8], signature: Option<&str>, timestamp: Option<&str>) -> bool {
        let (Some(sig), Some(ts)) = (signature, timestamp) else {
            return false;
        };
        let Some(hex_part) = sig.strip_prefix("v0=") else {
            return false;
        };
        let Ok(ts_secs) = ts.parse::<i64>() else {
            return false;
        };
        if (now_secs() - ts_secs).abs() > MAX_SKEW_SECS {
            return false; // stale or replayed
        }
        let Ok(expected) = hex::decode(hex_part) else {
            return false;
        };
        let Ok(mut mac) = HmacSha256::new_from_slice(self.signing_secret.as_bytes()) else {
            return false;
        };
        // Base string is exactly "v0:{timestamp}:{raw_body}".
        mac.update(b"v0:");
        mac.update(ts.as_bytes());
        mac.update(b":");
        mac.update(body);
        mac.verify_slice(&expected).is_ok() // constant-time
    }

    fn parse_webhook(&self, body: &[u8]) -> Result<Vec<MessageEvent>, GatewayError> {
        let value: Value =
            serde_json::from_slice(body).map_err(|e| GatewayError::Parse(e.to_string()))?;
        // url_verification challenges and other callbacks are handled at the route.
        if value.get("type").and_then(Value::as_str) != Some("event_callback") {
            return Ok(Vec::new());
        }
        let Some(event) = value.get("event") else {
            return Ok(Vec::new());
        };
        if event.get("type").and_then(Value::as_str) != Some("message") {
            return Ok(Vec::new());
        }
        // Skip bot messages and edits/joins (subtype) so the agent never echoes itself.
        if event.get("bot_id").is_some() || event.get("subtype").is_some() {
            return Ok(Vec::new());
        }
        let (Some(text), Some(channel)) = (
            event.get("text").and_then(Value::as_str),
            event.get("channel").and_then(Value::as_str),
        ) else {
            return Ok(Vec::new());
        };
        let user = event.get("user").and_then(Value::as_str).unwrap_or(channel);
        Ok(vec![MessageEvent {
            platform: "slack".to_owned(),
            chat_id: channel.to_owned(),
            user_id: user.to_owned(),
            text: text.to_owned(),
        }])
    }

    fn send_request(&self, message: &OutboundMessage) -> SendRequest {
        SendRequest {
            url: POST_MESSAGE_URL.to_owned(),
            auth: SendAuth::Bearer(self.bot_token.clone()),
            body: SendBody::Json(json!({"channel": message.chat_id, "text": message.text})),
        }
    }

    fn signature_header(&self) -> Option<&str> {
        Some("x-slack-signature")
    }

    fn timestamp_header(&self) -> Option<&str> {
        Some("x-slack-request-timestamp")
    }
}

#[cfg(test)]
mod tests {
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
        assert!(adapter.parse_webhook(bot).unwrap().is_empty(), "bot messages are ignored");

        let challenge = br#"{"type":"url_verification","challenge":"abc"}"#;
        assert!(adapter.parse_webhook(challenge).unwrap().is_empty());
    }

    #[test]
    fn send_request_posts_to_chat_postmessage() {
        let adapter = SlackAdapter::new("s", "BOT_TOKEN");
        let req = adapter.send_request(&OutboundMessage { chat_id: "C1".into(), text: "yo".into() });
        assert_eq!(req.url, POST_MESSAGE_URL);
        assert_eq!(req.auth, SendAuth::Bearer("BOT_TOKEN".into()));
        let SendBody::Json(body) = &req.body else { panic!("expected json body") };
        assert_eq!(body["channel"], "C1");
        assert_eq!(body["text"], "yo");
    }
}
