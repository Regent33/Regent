//! WhatsApp Cloud API webhook adapter. Like Messenger it's a Meta product, so
//! inbound POSTs are signed with `X-Hub-Signature-256` (HMAC-SHA256 of the raw
//! body, hex). Replies go out via the Cloud API messages endpoint. Parse/verify/
//! build are pure — unit-testable without a token.

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
                let messages = change.pointer("/value/messages").and_then(Value::as_array);
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

/// WhatsApp Cloud media send is two calls: upload the bytes to `/media` (returns
/// a media id), then send a message referencing that id. Both are pure to build
/// (tested below); only the two HTTP calls need the live client.
#[async_trait]
impl WebhookFileSender for WhatsAppAdapter {
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
        let mime = wa_mime_for(path);
        let msg_type = wa_message_type(mime);

        // 1. Upload the media → media id.
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name(filename.clone())
            .mime_str(mime)
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        let form = reqwest::multipart::Form::new()
            .text("messaging_product", "whatsapp")
            .text("type", mime.to_owned())
            .part("file", part);
        let upload_url = format!(
            "https://graph.facebook.com/v21.0/{}/media",
            self.phone_number_id
        );
        let resp = client
            .post(&upload_url)
            .bearer_auth(&self.access_token)
            .multipart(form)
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        let status = resp.status();
        let parsed: Value = resp
            .json()
            .await
            .map_err(|e| GatewayError::Parse(e.to_string()))?;
        let media_id = parsed.get("id").and_then(Value::as_str).ok_or_else(|| {
            GatewayError::Transport(format!("whatsapp media upload failed ({status}): {parsed}"))
        })?;

        // 2. Send the message referencing the uploaded media id.
        let body = wa_media_body(chat_id, media_id, msg_type, &filename, caption);
        let msg_url = format!(
            "https://graph.facebook.com/v21.0/{}/messages",
            self.phone_number_id
        );
        client
            .post(&msg_url)
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        Ok(())
    }
}

/// Best-effort extension → MIME for the multipart upload's `Content-Type` (the
/// Cloud API rejects a wrong type). Common chat attachments only.
// ponytail: extension map; swap for the `mime_guess` crate if the long tail matters.
fn wa_mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "3gp" => "video/3gpp",
        "mp3" => "audio/mpeg",
        "ogg" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        "amr" => "audio/amr",
        "aac" => "audio/aac",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "csv" => "text/csv",
        "json" => "application/json",
        "zip" => "application/zip",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        _ => "application/octet-stream",
    }
}

/// The Cloud API message `type` (and body key) for a MIME type: image/audio/
/// video get their own, everything else is a `document`.
fn wa_message_type(mime: &str) -> &'static str {
    if mime.starts_with("image/") {
        "image"
    } else if mime.starts_with("video/") {
        "video"
    } else {
        // Everything else (incl. arbitrary audio — voice notes are a separate
        // path) rides as a document so the filename + caption survive.
        "document"
    }
}

/// Builds the `/messages` body that sends an already-uploaded media id. Captions
/// ride on image/video/document (not audio); `filename` rides on documents.
fn wa_media_body(to: &str, media_id: &str, msg_type: &str, filename: &str, caption: &str) -> Value {
    let mut media = json!({ "id": media_id });
    if !caption.is_empty() && msg_type != "audio" {
        media["caption"] = json!(caption);
    }
    if msg_type == "document" {
        media["filename"] = json!(filename);
    }
    json!({
        "messaging_product": "whatsapp",
        "to": to,
        "type": msg_type,
        msg_type: media,
    })
}

#[cfg(test)]
#[path = "whatsapp_tests.rs"]
mod tests;
