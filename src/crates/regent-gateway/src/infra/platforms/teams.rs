//! Microsoft Teams **Outgoing Webhook** adapter. When a user @mentions the
//! webhook, Teams POSTs a Bot Framework activity with
//! `Authorization: HMAC <base64(HMAC-SHA256(body, key))>`, where `key` is the
//! base64-decoded security token shown at registration. The reply is returned
//! **synchronously** in the HTTP response body (`{"type":"message",...}`), so
//! this adapter sets `sync_reply` and never uses `send_request`. Parse/verify
//! are pure — unit-testable without a live tenant.
//!
//! (This is the shared-secret Outgoing Webhook, distinct from the Bot Framework
//! / Azure Bot Service path, which authenticates with a rotating JWT.)

use crate::domain::contracts::{SendAuth, SendBody, SendRequest, SyncReply, WebhookAdapter};
use crate::domain::entities::{MessageEvent, OutboundMessage};
use crate::domain::errors::GatewayError;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub struct TeamsAdapter {
    /// The base64 security token from the Outgoing Webhook registration.
    security_token: String,
}

impl TeamsAdapter {
    #[must_use]
    pub fn new(security_token: impl Into<String>) -> Self {
        Self {
            security_token: security_token.into(),
        }
    }
}

/// Removes `<at>…</at>` mention markup Teams wraps the bot name in.
fn strip_mentions(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<at>") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</at>") {
            Some(end) => rest = &rest[start + end + "</at>".len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out.trim().to_owned()
}

impl WebhookAdapter for TeamsAdapter {
    fn platform(&self) -> &str {
        "teams"
    }

    fn verify(&self, body: &[u8], signature: Option<&str>, _timestamp: Option<&str>) -> bool {
        let Some(sig) = signature.and_then(|s| s.strip_prefix("HMAC ")) else {
            return false;
        };
        let Ok(expected) = STANDARD.decode(sig) else {
            return false;
        };
        let Ok(key) = STANDARD.decode(&self.security_token) else {
            return false;
        };
        let Ok(mut mac) = HmacSha256::new_from_slice(&key) else {
            return false;
        };
        mac.update(body);
        mac.verify_slice(&expected).is_ok() // constant-time
    }

    fn parse_webhook(&self, body: &[u8]) -> Result<Vec<MessageEvent>, GatewayError> {
        let value: Value =
            serde_json::from_slice(body).map_err(|e| GatewayError::Parse(e.to_string()))?;
        if value.get("type").and_then(Value::as_str) != Some("message") {
            return Ok(Vec::new());
        }
        let Some(raw) = value.get("text").and_then(Value::as_str) else {
            return Ok(Vec::new());
        };
        let text = strip_mentions(raw);
        if text.is_empty() {
            return Ok(Vec::new());
        }
        let user = value
            .pointer("/from/id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let chat = value
            .pointer("/conversation/id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or(user);
        Ok(vec![MessageEvent {
            platform: "teams".to_owned(),
            chat_id: chat.to_owned(),
            user_id: user.to_owned(),
            text,
        }])
    }

    fn send_request(&self, _message: &OutboundMessage) -> SendRequest {
        // Teams Outgoing Webhooks reply synchronously (see `sync_reply`); the
        // route never calls this for this adapter.
        SendRequest {
            url: String::new(),
            auth: SendAuth::None,
            body: SendBody::Json(Value::Null),
        }
    }

    fn signature_header(&self) -> Option<&str> {
        Some("authorization")
    }

    fn sync_reply(&self) -> bool {
        true
    }

    fn sync_response(&self, reply: &str) -> SyncReply {
        SyncReply::Json(json!({ "type": "message", "text": reply }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token() -> String {
        STANDARD.encode(b"super-secret-key-bytes")
    }

    fn sign(token_b64: &str, body: &[u8]) -> String {
        let key = STANDARD.decode(token_b64).unwrap();
        let mut mac = HmacSha256::new_from_slice(&key).unwrap();
        mac.update(body);
        format!("HMAC {}", STANDARD.encode(mac.finalize().into_bytes()))
    }

    #[test]
    fn verifies_hmac_and_rejects_tampering() {
        let adapter = TeamsAdapter::new(token());
        let body = br#"{"type":"message","text":"hi"}"#;
        assert!(adapter.verify(body, Some(&sign(&token(), body)), None));
        // Signature computed over a different body.
        assert!(!adapter.verify(body, Some(&sign(&token(), b"other")), None));
        // Wrong key (different token).
        assert!(!adapter.verify(body, Some(&sign(&STANDARD.encode(b"nope"), body)), None));
        // Missing `HMAC ` prefix / missing header / garbage.
        assert!(!adapter.verify(body, Some(&STANDARD.encode(b"x")), None));
        assert!(!adapter.verify(body, None, None));
        assert!(!adapter.verify(body, Some("HMAC !!!notb64"), None));
    }

    #[test]
    fn parses_message_and_strips_mention() {
        let adapter = TeamsAdapter::new(token());
        let body = br#"{"type":"message","text":"<at>RegentBot</at> hello there",
            "from":{"id":"29:u1"},"conversation":{"id":"19:abc@thread.tacv2"}}"#;
        let events = adapter.parse_webhook(body).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].text, "hello there");
        assert_eq!(events[0].chat_id, "19:abc@thread.tacv2");
        assert_eq!(events[0].user_id, "29:u1");

        // Non-message activities (typing, etc.) and mention-only text are skipped.
        assert!(
            adapter
                .parse_webhook(br#"{"type":"typing"}"#)
                .unwrap()
                .is_empty()
        );
        let mention_only = br#"{"type":"message","text":"<at>RegentBot</at>"}"#;
        assert!(adapter.parse_webhook(mention_only).unwrap().is_empty());
    }

    #[test]
    fn sync_reply_renders_a_teams_message() {
        let adapter = TeamsAdapter::new(token());
        assert!(adapter.sync_reply());
        let SyncReply::Json(body) = adapter.sync_response("the reply") else {
            panic!("expected a json body");
        };
        assert_eq!(body["type"], "message");
        assert_eq!(body["text"], "the reply");
    }
}
