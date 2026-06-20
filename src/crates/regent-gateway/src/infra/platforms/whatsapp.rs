//! WhatsApp Cloud API webhook adapter. Like Messenger it's a Meta product, so
//! inbound POSTs are signed with `X-Hub-Signature-256` (HMAC-SHA256 of the raw
//! body, hex). Replies go out via the Cloud API messages endpoint. Parse/verify/
//! build are pure — unit-testable without a token.

use crate::domain::contracts::{SendAuth, SendBody, SendRequest, WebhookAdapter};
use crate::domain::entities::{MessageEvent, OutboundMessage};
use crate::domain::errors::GatewayError;
use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub struct WhatsAppAdapter {
    app_secret: String,
    access_token: String,
    phone_number_id: String,
}

impl WhatsAppAdapter {
    #[must_use]
    pub fn new(
        app_secret: impl Into<String>,
        access_token: impl Into<String>,
        phone_number_id: impl Into<String>,
    ) -> Self {
        Self {
            app_secret: app_secret.into(),
            access_token: access_token.into(),
            phone_number_id: phone_number_id.into(),
        }
    }
}

impl WebhookAdapter for WhatsAppAdapter {
    fn platform(&self) -> &str {
        "whatsapp"
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
            let changes = entry.get("changes").and_then(Value::as_array);
            for change in changes.into_iter().flatten() {
                let messages =
                    change.pointer("/value/messages").and_then(Value::as_array);
                for msg in messages.into_iter().flatten() {
                    let (Some(from), Some(text)) = (
                        msg.get("from").and_then(Value::as_str),
                        msg.pointer("/text/body").and_then(Value::as_str),
                    ) else {
                        continue; // status callbacks / non-text messages
                    };
                    events.push(MessageEvent {
                        platform: "whatsapp".to_owned(),
                        chat_id: from.to_owned(),
                        user_id: from.to_owned(),
                        text: text.to_owned(),
                    });
                }
            }
        }
        Ok(events)
    }

    fn send_request(&self, message: &OutboundMessage) -> SendRequest {
        SendRequest {
            url: format!(
                "https://graph.facebook.com/v21.0/{}/messages",
                self.phone_number_id
            ),
            auth: SendAuth::Bearer(self.access_token.clone()),
            body: SendBody::Json(json!({
                "messaging_product": "whatsapp",
                "to": message.chat_id,
                "type": "text",
                "text": {"body": message.text},
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
        let adapter = WhatsAppAdapter::new("app-secret", "tok", "PHONE");
        let body = br#"{"object":"whatsapp_business_account"}"#;
        assert!(adapter.verify(body, Some(&sign("app-secret", body)), None));
        assert!(!adapter.verify(body, Some("sha256=deadbeef"), None));
        assert!(!adapter.verify(body, None, None));
        assert!(!adapter.verify(body, Some(&sign("wrong", body)), None));
    }

    #[test]
    fn parses_text_messages_and_skips_status_callbacks() {
        let adapter = WhatsAppAdapter::new("s", "t", "PHONE");
        let body = br#"{"entry":[{"changes":[
            {"value":{"messages":[{"from":"15551234","type":"text","text":{"body":"hi"}}]}},
            {"value":{"statuses":[{"status":"delivered"}]}}
        ]}]}"#;
        let events = adapter.parse_webhook(body).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].chat_id, "15551234");
        assert_eq!(events[0].text, "hi");
        assert_eq!(events[0].user_key(), "whatsapp:15551234");
    }

    #[test]
    fn send_request_targets_the_cloud_api() {
        let adapter = WhatsAppAdapter::new("s", "WA_TOKEN", "PHONE42");
        let req = adapter.send_request(&OutboundMessage { chat_id: "15551234".into(), text: "hey".into() });
        assert_eq!(req.url, "https://graph.facebook.com/v21.0/PHONE42/messages");
        assert_eq!(req.auth, SendAuth::Bearer("WA_TOKEN".into()));
        let SendBody::Json(body) = &req.body else { panic!("expected json body") };
        assert_eq!(body["messaging_product"], "whatsapp");
        assert_eq!(body["to"], "15551234");
        assert_eq!(body["text"]["body"], "hey");
    }
}
