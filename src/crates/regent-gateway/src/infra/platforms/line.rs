//! LINE Messaging API webhook adapter. Inbound events arrive as POSTs signed
//! with `X-Line-Signature` (base64 of HMAC-SHA256 over the raw body). Replies
//! go out via the push API. Parse/verify/build are pure — unit-testable.

use crate::domain::contracts::{SendAuth, SendBody, SendRequest, WebhookAdapter};
use crate::domain::entities::{MessageEvent, OutboundMessage};
use crate::domain::errors::GatewayError;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

const PUSH_URL: &str = "https://api.line.me/v2/bot/message/push";

pub struct LineAdapter {
    channel_secret: String,
    channel_access_token: String,
}

impl LineAdapter {
    #[must_use]
    pub fn new(channel_secret: impl Into<String>, channel_access_token: impl Into<String>) -> Self {
        Self { channel_secret: channel_secret.into(), channel_access_token: channel_access_token.into() }
    }
}

impl WebhookAdapter for LineAdapter {
    fn platform(&self) -> &str {
        "line"
    }

    fn verify(&self, body: &[u8], signature: Option<&str>, _timestamp: Option<&str>) -> bool {
        let Some(sig) = signature else { return false };
        let Ok(expected) = STANDARD.decode(sig) else {
            return false;
        };
        let Ok(mut mac) = HmacSha256::new_from_slice(self.channel_secret.as_bytes()) else {
            return false;
        };
        mac.update(body);
        mac.verify_slice(&expected).is_ok() // constant-time
    }

    fn parse_webhook(&self, body: &[u8]) -> Result<Vec<MessageEvent>, GatewayError> {
        let value: Value =
            serde_json::from_slice(body).map_err(|e| GatewayError::Parse(e.to_string()))?;
        let mut events = Vec::new();
        let entries = value.get("events").and_then(Value::as_array);
        for event in entries.into_iter().flatten() {
            if event.get("type").and_then(Value::as_str) != Some("message") {
                continue;
            }
            let Some(text) = event.pointer("/message/text").and_then(Value::as_str) else {
                continue; // non-text message (sticker/image/…)
            };
            // Route on the most specific source id (group/room/user).
            let source = event.get("source");
            let Some(id) = source
                .and_then(|s| {
                    s.get("groupId").or_else(|| s.get("roomId")).or_else(|| s.get("userId"))
                })
                .and_then(Value::as_str)
            else {
                continue;
            };
            let user = source
                .and_then(|s| s.get("userId"))
                .and_then(Value::as_str)
                .unwrap_or(id);
            events.push(MessageEvent {
                platform: "line".to_owned(),
                chat_id: id.to_owned(),
                user_id: user.to_owned(),
                text: text.to_owned(),
            });
        }
        Ok(events)
    }

    fn send_request(&self, message: &OutboundMessage) -> SendRequest {
        SendRequest {
            url: PUSH_URL.to_owned(),
            auth: SendAuth::Bearer(self.channel_access_token.clone()),
            body: SendBody::Json(json!({
                "to": message.chat_id,
                "messages": [{"type": "text", "text": message.text}],
            })),
        }
    }

    fn signature_header(&self) -> Option<&str> {
        Some("x-line-signature")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sign(secret: &str, body: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        STANDARD.encode(mac.finalize().into_bytes())
    }

    #[test]
    fn verifies_a_valid_signature_and_rejects_others() {
        let adapter = LineAdapter::new("chan-secret", "tok");
        let body = br#"{"events":[]}"#;
        assert!(adapter.verify(body, Some(&sign("chan-secret", body)), None));
        assert!(!adapter.verify(body, Some("not-base64-!!"), None));
        assert!(!adapter.verify(body, None, None));
        assert!(!adapter.verify(body, Some(&sign("wrong", body)), None));
    }

    #[test]
    fn parses_text_message_events_and_prefers_group_id() {
        let adapter = LineAdapter::new("s", "t");
        let body = br#"{"events":[
            {"type":"message","message":{"type":"text","text":"hi"},
             "source":{"type":"group","groupId":"G1","userId":"U9"}},
            {"type":"message","message":{"type":"sticker"},"source":{"userId":"U2"}},
            {"type":"follow","source":{"userId":"U3"}}
        ]}"#;
        let events = adapter.parse_webhook(body).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].chat_id, "G1", "route on the group, not the user");
        assert_eq!(events[0].user_id, "U9");
        assert_eq!(events[0].text, "hi");
    }

    #[test]
    fn send_request_targets_the_push_api() {
        let adapter = LineAdapter::new("s", "CHAN_TOKEN");
        let req = adapter.send_request(&OutboundMessage { chat_id: "G1".into(), text: "yo".into() });
        assert_eq!(req.url, PUSH_URL);
        assert_eq!(req.auth, SendAuth::Bearer("CHAN_TOKEN".into()));
        let SendBody::Json(body) = &req.body else { panic!("expected json body") };
        assert_eq!(body["to"], "G1");
        assert_eq!(body["messages"][0]["text"], "yo");
    }
}
