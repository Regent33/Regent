//! Facebook Messenger webhook adapter. Inbound events arrive as POSTs signed
//! with `X-Hub-Signature-256` (HMAC-SHA256 of the raw body, hex). Parse/verify/
//! build are pure — unit-testable without a token; only the send is live.

use crate::domain::contracts::{
    SendAuth, SendBody, SendRequest, WebhookAdapter, WebhookFileSender,
};
use crate::domain::entities::{MessageEvent, OutboundMessage};
use crate::domain::errors::GatewayError;
use async_trait::async_trait;
use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;
use std::path::Path;

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

/// Messenger file send: one multipart POST to the Send API with the bytes as
/// `filedata` and an attachment message; the attachment carries no text, so a
/// non-empty caption is sent as a follow-up text message.
#[async_trait]
impl WebhookFileSender for MessengerAdapter {
    async fn send_file(
        &self,
        client: &reqwest::Client,
        chat_id: &str,
        path: &Path,
        caption: &str,
    ) -> Result<(), GatewayError> {
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|e| GatewayError::Transport(format!("read {}: {e}", path.display())))?;
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_owned();
        let recipient = json!({ "id": chat_id }).to_string();
        let message = json!({
            "attachment": { "type": attachment_kind(path), "payload": { "is_reusable": false } }
        })
        .to_string();
        let part = reqwest::multipart::Part::bytes(bytes).file_name(filename);
        let form = reqwest::multipart::Form::new()
            .text("recipient", recipient)
            .text("message", message)
            .part("filedata", part);
        client
            .post(GRAPH_SEND_URL)
            .bearer_auth(&self.page_access_token)
            .multipart(form)
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        if !caption.is_empty() {
            let _ = client
                .post(GRAPH_SEND_URL)
                .bearer_auth(&self.page_access_token)
                .json(&json!({
                    "recipient": { "id": chat_id },
                    "messaging_type": "RESPONSE",
                    "message": { "text": caption },
                }))
                .send()
                .await;
        }
        Ok(())
    }
}

/// Messenger attachment type for a local file, by extension (it has image /
/// video / audio / file categories).
fn attachment_kind(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" | "jpg" | "jpeg" | "gif" | "webp" => "image",
        "mp4" | "mov" | "webm" => "video",
        "mp3" | "wav" | "ogg" | "m4a" | "aac" => "audio",
        _ => "file",
    }
}

#[cfg(test)]
#[path = "messenger_tests.rs"]
mod tests;
