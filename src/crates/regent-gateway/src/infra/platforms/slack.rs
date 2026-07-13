//! Slack Events API webhook adapter. Slack signs the base string
//! `v0:{timestamp}:{body}` with the app signing secret (HMAC-SHA256, hex),
//! delivered as `X-Slack-Signature` with the timestamp in
//! `X-Slack-Request-Timestamp`. Verification also rejects stale timestamps (a
//! replay window). Replies go out via chat.postMessage. Parse/build are pure;
//! verify touches the wall clock only for the replay check.

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
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

const POST_MESSAGE_URL: &str = "https://slack.com/api/chat.postMessage";
const GET_UPLOAD_URL: &str = "https://slack.com/api/files.getUploadURLExternal";
const COMPLETE_UPLOAD_URL: &str = "https://slack.com/api/files.completeUploadExternal";
/// Slack's recommended replay window.
const MAX_SKEW_SECS: i64 = 60 * 5;

pub struct SlackAdapter {
    signing_secret: String,
    bot_token: String,
}

impl SlackAdapter {
    #[must_use]
    pub fn new(signing_secret: impl Into<String>, bot_token: impl Into<String>) -> Self {
        Self {
            signing_secret: signing_secret.into(),
            bot_token: bot_token.into(),
        }
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

/// Slack's three-step upload (the post-`files.upload` flow): reserve an upload
/// URL → PUT the bytes there → complete, which posts the file to the channel.
/// Only the request/response shapes are pure (tested); the three calls run on
/// the injected client.
#[async_trait]
impl WebhookFileSender for SlackAdapter {
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
        let length = bytes.len().to_string();

        // 1. Reserve an upload URL + file id (form params, POST).
        let resp = client
            .post(GET_UPLOAD_URL)
            .bearer_auth(&self.bot_token)
            .form(&[("filename", filename.as_str()), ("length", length.as_str())])
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        let reserved: Value = resp
            .json()
            .await
            .map_err(|e| GatewayError::Parse(e.to_string()))?;
        if reserved.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(GatewayError::Transport(format!(
                "slack getUploadURLExternal failed: {reserved}"
            )));
        }
        let (Some(upload_url), Some(file_id)) = (
            reserved.get("upload_url").and_then(Value::as_str),
            reserved.get("file_id").and_then(Value::as_str),
        ) else {
            return Err(GatewayError::Transport(format!(
                "slack getUploadURLExternal missing url/id: {reserved}"
            )));
        };

        // 2. Upload the bytes to the reserved URL.
        let part = reqwest::multipart::Part::bytes(bytes).file_name(filename);
        let form = reqwest::multipart::Form::new().part("file", part);
        client
            .post(upload_url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;

        // 3. Complete → publishes the file to the channel.
        let body = slack_complete_body(file_id, chat_id, caption);
        let resp = client
            .post(COMPLETE_UPLOAD_URL)
            .bearer_auth(&self.bot_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        let done: Value = resp
            .json()
            .await
            .map_err(|e| GatewayError::Parse(e.to_string()))?;
        if done.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(GatewayError::Transport(format!(
                "slack completeUploadExternal failed: {done}"
            )));
        }
        Ok(())
    }
}

/// Body for `files.completeUploadExternal`: the reserved file id, the target
/// channel, and the caption as the file's initial comment (omitted when empty).
fn slack_complete_body(file_id: &str, channel: &str, comment: &str) -> Value {
    let mut body = json!({
        "files": [{ "id": file_id }],
        "channel_id": channel,
    });
    if !comment.is_empty() {
        body["initial_comment"] = json!(comment);
    }
    body
}

#[cfg(test)]
#[path = "slack_tests.rs"]
mod tests;
