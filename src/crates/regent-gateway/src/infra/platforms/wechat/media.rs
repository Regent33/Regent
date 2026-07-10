//! WeChat Official Account media send: upload a temporary media (image/voice/
//! video — the OA API has no generic "document") → send it by `media_id` via the
//! Customer Service API. Needs the operator's `access_token`; a caption rides as
//! a preceding text message (media messages carry no caption field).

use super::WeChatAdapter;
use crate::domain::contracts::WebhookFileSender;
use crate::domain::errors::GatewayError;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::path::Path;

#[async_trait]
impl WebhookFileSender for WeChatAdapter {
    async fn send_file(
        &self,
        client: &reqwest::Client,
        chat_id: &str,
        path: &Path,
        caption: &str,
    ) -> Result<(), GatewayError> {
        let Some(token) = self.access_token() else {
            return Err(GatewayError::Transport(
                "wechat: no access_token configured for media send".to_owned(),
            ));
        };
        let Some(kind) = wechat_media_kind(path) else {
            return Err(GatewayError::Transport(format!(
                "wechat: unsupported media type for {} (only image/voice/video)",
                path.display()
            )));
        };
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|e| GatewayError::Transport(format!("read {}: {e}", path.display())))?;
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_owned();

        // 1. Upload temporary media → media_id.
        let upload_url = format!(
            "https://api.weixin.qq.com/cgi-bin/media/upload?access_token={token}&type={kind}"
        );
        let part = reqwest::multipart::Part::bytes(bytes).file_name(filename);
        let form = reqwest::multipart::Form::new().part("media", part);
        let resp = client
            .post(&upload_url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        let parsed: Value = resp
            .json()
            .await
            .map_err(|e| GatewayError::Parse(e.to_string()))?;
        let media_id = parsed
            .get("media_id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                GatewayError::Transport(format!("wechat media upload failed: {parsed}"))
            })?;

        let send_url =
            format!("https://api.weixin.qq.com/cgi-bin/message/custom/send?access_token={token}");
        // 2. Optional caption first (media messages have no caption field).
        if !caption.is_empty() {
            let text =
                json!({ "touser": chat_id, "msgtype": "text", "text": { "content": caption } });
            let _ = client.post(&send_url).json(&text).send().await;
        }
        // 3. Send the media by id.
        let body = wechat_media_body(chat_id, kind, media_id);
        client
            .post(&send_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        Ok(())
    }
}

/// The OA media category for a file, or `None` for unsupported types. WeChat's
/// temporary-media API only accepts image/voice/video (no arbitrary documents).
pub(super) fn wechat_media_kind(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" => Some("image"),
        "mp3" | "amr" | "wav" | "ogg" | "opus" | "m4a" => Some("voice"),
        "mp4" => Some("video"),
        _ => None,
    }
}

/// Customer Service body sending an uploaded media by id: `{touser, msgtype,
/// <msgtype>: { media_id }}` (works for image/voice/video).
pub(super) fn wechat_media_body(touser: &str, kind: &str, media_id: &str) -> Value {
    json!({
        "touser": touser,
        "msgtype": kind,
        kind: { "media_id": media_id },
    })
}
