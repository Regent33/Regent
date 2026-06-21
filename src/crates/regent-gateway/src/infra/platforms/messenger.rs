//! Facebook Messenger webhook adapter. Inbound events arrive as POSTs signed
//! with `X-Hub-Signature-256` (HMAC-SHA256 of the raw body, hex). Parse/verify/
//! build are pure — unit-testable without a token; only the send is live.

use crate::domain::contracts::{SendAuth, SendBody, SendRequest, WebhookAdapter};
use crate::domain::entities::{MessageEvent, OutboundMessage};
use crate::domain::errors::GatewayError;
use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

const GRAPH_SEND_URL: &str = "https://graph.facebook.com/v21.0/me/messages";

pub struct MessengerAdapter {
    app_secret: String,
    page_access_token: String,
}

impl MessengerAdapter {
    #[must_use]
    pub fn new(app_secret: impl Into<String>, page_access_token: impl Into<String>) -> Self {
        Self {
            app_secret: app_secret.into(),
            page_access_token: page_access_token.into(),
        }
    }
}

impl WebhookAdapter for MessengerAdapter {
    fn platform(&self) -> &str {
        "messenger"
    }

    fn verify(&self, body: &[u8], signature: Option<&str>, _timestamp: Option<&str>) -> bool {
        let Some(hex_part) = signature.and_then(|s| s.strip_prefix("sha256=")) else {
            return false;
        };
        let Ok(expected) = hex::decode(hex_part) else {
            return false;
        };
        let Ok(mut mac) = HmacSha256::new_from_slice(self.app_secret.as_bytes()) else {
            return false;
        };
        mac.update(body);
        mac.verify_slice(&expected).is_ok() // constant-time
    }

    fn parse_webhook(&self, body: &[u8]) -> Result<Vec<MessageEvent>, GatewayError> {
        let value: Value =
            serde_json::from_slice(body).map_err(|e| GatewayError::Parse(e.to_string()))?;
        let mut events = Vec::new();
        let entries = value.get("entry").and_then(Value::as_array);
        for entry in entries.into_iter().flatten() {
            let messaging = entry.get("messaging").and_then(Value::as_array);
            for m in messaging.into_iter().flatten() {
                let (Some(sender), Some(text)) = (
                    m.pointer("/sender/id").and_then(Value::as_str),
                    m.pointer("/message/text").and_then(Value::as_str),
                ) else {
                    continue; // skip deliveries/reads/non-text
                };
                events.push(MessageEvent {
                    platform: "messenger".to_owned(),
                    chat_id: sender.to_owned(),
                    user_id: sender.to_owned(),
                    text: text.to_owned(),
                });
            }
        }
        Ok(events)
    }

    fn send_request(&self, message: &OutboundMessage) -> SendRequest {
        SendRequest {
            url: GRAPH_SEND_URL.to_owned(),
            auth: SendAuth::Bearer(self.page_access_token.clone()),
            body: SendBody::Json(json!({
                "recipient": {"id": message.chat_id},
                "messaging_type": "RESPONSE",
                "message": {"text": message.text},
            })),
        }
    }

    fn signature_header(&self) -> Option<&str> {
        Some("x-hub-signature-256")
    }
}

#[cfg(test)]
mod tests {
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
}
