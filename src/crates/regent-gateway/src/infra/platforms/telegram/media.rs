//! Inbound attachments: photos, documents and video sent TO the bot.
//!
//! Text and voice used to be the only shapes `next_event` understood, so a
//! photo update matched nothing and was dropped by the poll loop — the user
//! sent a screenshot and Regent never knew a message had arrived at all.
//! Here the file is downloaded into the artifacts area and handed to the agent
//! as a path in the turn text, which is exactly what `vision_analyze` /
//! `read_document` take.

use super::TelegramAdapter;
use crate::domain::errors::GatewayError;
use serde_json::Value;
use std::path::PathBuf;

/// One inbound attachment, before download.
pub(super) struct Attachment {
    pub chat_id: String,
    pub user_id: String,
    pub file_id: String,
    /// The sender's message text alongside the file, if any.
    pub caption: String,
    /// Sender-provided name; photos have none.
    pub file_name: Option<String>,
}

impl TelegramAdapter {
    /// Download an attachment and describe it for the agent as a turn of text.
    /// Never fails the poll loop: a download error becomes a note the model can
    /// relay, because dropping the turn is what caused the original silence.
    pub(super) async fn receive_attachment(&self, item: &Attachment) -> String {
        let caption = item.caption.trim();
        match self.save_attachment(item).await {
            Ok(path) => {
                let intro = if caption.is_empty() {
                    "The user sent you a file."
                } else {
                    "The user sent you a file with this message:"
                };
                // Naming the tools here is deliberate: without it a model that
                // CAN see the file still tends to answer "I can't open files".
                format!(
                    "{intro} {caption}\n\n[attachment saved to: {}]\nLook at it yourself before \
                     replying — vision_analyze for images, read_document for PDFs/Office files.",
                    path.display()
                )
            }
            Err(error) => {
                tracing::warn!(%error, "attachment download failed");
                let named = item.file_name.as_deref().unwrap_or("the file");
                format!(
                    "The user tried to send {named}{}{caption}, but it couldn't be downloaded \
                     ({error}). Tell them, and suggest resending it or sharing a path.",
                    if caption.is_empty() { "" } else { " with: " },
                )
            }
        }
    }

    /// Download into `<artifacts>/inbox/`, the one place a chat surface may
    /// write and `send_file` may read from.
    async fn save_attachment(&self, item: &Attachment) -> Result<PathBuf, GatewayError> {
        let bytes = self.download_file(&item.file_id).await?;
        let dir = artifacts_inbox();
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| GatewayError::Transport(format!("create {}: {e}", dir.display())))?;
        let path = dir.join(unique_name(item.file_name.as_deref(), &item.file_id));
        tokio::fs::write(&path, &bytes)
            .await
            .map_err(|e| GatewayError::Transport(format!("write {}: {e}", path.display())))?;
        Ok(path)
    }
}

fn artifacts_inbox() -> PathBuf {
    let home = std::env::var("REGENT_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let base = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_default();
            PathBuf::from(base).join(".regent")
        });
    home.join("artifacts").join("inbox")
}

/// A collision-free, path-safe file name. The Telegram file_id suffix keeps two
/// `photo.jpg`s apart; stripping directory separators keeps a hostile name from
/// escaping the inbox.
fn unique_name(file_name: Option<&str>, file_id: &str) -> String {
    let tag: String = file_id
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .collect();
    let raw = file_name.unwrap_or("photo.jpg");
    let safe: String = raw
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || "._- ".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let safe = safe.trim_matches(['.', ' ']).to_owned();
    let safe = if safe.is_empty() {
        "file".to_owned()
    } else {
        safe
    };
    match safe.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem}-{tag}.{ext}"),
        None => format!("{safe}-{tag}"),
    }
}

/// Extracts every inbound photo / document / video attachment. Pure — the
/// download happens in `next_event`.
#[must_use]
pub(super) fn parse_attachments(body: &Value) -> Vec<Attachment> {
    let Some(updates) = body.get("result").and_then(Value::as_array) else {
        return Vec::new();
    };
    updates
        .iter()
        .filter_map(|update| {
            let message = update.get("message")?;
            // Photos arrive as a size ladder; the last entry is the largest,
            // which is the one worth looking at.
            let photo = message
                .get("photo")
                .and_then(Value::as_array)
                .and_then(|sizes| sizes.last());
            let file = photo
                .or_else(|| message.get("document"))
                .or_else(|| message.get("video"))?;
            Some(Attachment {
                chat_id: message.pointer("/chat/id")?.as_i64()?.to_string(),
                user_id: message.pointer("/from/id")?.as_i64()?.to_string(),
                file_id: file.get("file_id")?.as_str()?.to_owned(),
                caption: message
                    .get("caption")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                file_name: file
                    .get("file_name")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "media_tests.rs"]
mod tests;
