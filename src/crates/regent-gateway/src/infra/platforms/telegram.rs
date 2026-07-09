//! Telegram Bot API adapter (long polling). Wire parse/build are pure functions
//! (unit-testable without a token). Turn-based voice — transcribe inbound notes,
//! speak replies — lives in the [`voice`] submodule, so `MessageEvent` and the
//! runner stay text-only.

mod voice;
mod wire;

pub use wire::{parse_updates, send_payload};

use crate::domain::contracts::PlatformAdapter;
use crate::domain::entities::{MessageEvent, OutboundMessage};
use crate::domain::errors::GatewayError;
use async_trait::async_trait;
use regent_kernel::{AsrProvider, TtsProvider};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const LONG_POLL_SECS: u64 = 30;

pub struct TelegramAdapter {
    token: String,
    client: reqwest::Client,
    /// Highest update_id seen; getUpdates offset = this + 1.
    offset: AtomicI64,
    /// Speech for voice notes/replies; `None` until `with_speech` (see [`voice`]).
    asr: Option<Arc<dyn AsrProvider>>,
    tts: Option<Arc<dyn TtsProvider>>,
    /// Chats whose last inbound was voice → reply is spoken (cleared on text).
    voice_chats: Mutex<HashSet<String>>,
}

impl TelegramAdapter {
    #[must_use]
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            client: reqwest::Client::new(),
            offset: AtomicI64::new(0),
            asr: None,
            tts: None,
            voice_chats: Mutex::new(HashSet::new()),
        }
    }

    /// Enable voice: inbound notes are transcribed by `asr`, and replies in a
    /// chat that spoke are synthesized by `tts` and sent as a voice bubble.
    #[must_use]
    pub fn with_speech(mut self, asr: Arc<dyn AsrProvider>, tts: Arc<dyn TtsProvider>) -> Self {
        self.asr = Some(asr);
        self.tts = Some(tts);
        self
    }

    fn url(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{method}", self.token)
    }

    async fn call(&self, method: &str, payload: Value) -> Result<Value, GatewayError> {
        let response = self
            .client
            .post(self.url(method))
            .timeout(Duration::from_secs(LONG_POLL_SECS + 15))
            .json(&payload)
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        let body: Value = response
            .json()
            .await
            .map_err(|e| GatewayError::Parse(e.to_string()))?;
        if body.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(GatewayError::Transport(format!(
                "telegram API error: {body}"
            )));
        }
        Ok(body)
    }
}

#[async_trait]
impl PlatformAdapter for TelegramAdapter {
    fn platform(&self) -> &str {
        "telegram"
    }

    async fn next_event(&self) -> Result<MessageEvent, GatewayError> {
        loop {
            let offset = self.offset.load(Ordering::Relaxed) + 1;
            let body = self
                .call(
                    "getUpdates",
                    json!({"offset": offset, "timeout": LONG_POLL_SECS}),
                )
                .await?;
            // Advance past everything we saw, message or not (e.g. edits).
            if let Some(max_id) = body
                .get("result")
                .and_then(Value::as_array)
                .and_then(|updates| {
                    updates
                        .iter()
                        .filter_map(|u| u.get("update_id")?.as_i64())
                        .max()
                })
            {
                self.offset.fetch_max(max_id, Ordering::Relaxed);
            }
            // Text turns first; a chat that types again leaves voice mode.
            if let Some(event) = parse_updates(&body).into_iter().next() {
                self.clear_voice(&event.chat_id);
                return Ok(event);
            }
            // Then voice notes → transcribe → text turn, remembering the chat
            // spoke so its reply is spoken back.
            if let Some((chat_id, user_id, file_id)) = voice::parse_voice(&body).into_iter().next()
            {
                let text = self.transcribe_voice(&file_id).await;
                self.mark_voice(&chat_id);
                return Ok(MessageEvent {
                    platform: "telegram".to_owned(),
                    chat_id,
                    user_id,
                    text,
                });
            }
            // empty long-poll → poll again
        }
    }

    async fn send(&self, message: OutboundMessage) -> Result<(), GatewayError> {
        // Spoken reply when the chat last sent voice and TTS is wired; fall back
        // to text on any failure so a reply always lands.
        if self.is_voice_chat(&message.chat_id) && self.tts.is_some() {
            match self.send_voice_reply(&message).await {
                Ok(()) => return Ok(()),
                Err(error) => tracing::warn!(%error, "voice reply failed; sending text"),
            }
        }
        self.call("sendMessage", send_payload(&message)).await?;
        Ok(())
    }

    async fn send_typing(&self, chat_id: &str) -> Result<(), GatewayError> {
        // Best-effort: a failed indicator must never break the turn.
        let _ = self
            .call(
                "sendChatAction",
                json!({"chat_id": chat_id, "action": "typing"}),
            )
            .await;
        Ok(())
    }

    async fn send_file(
        &self,
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
        let part = reqwest::multipart::Part::bytes(bytes).file_name(filename);
        let mut form = reqwest::multipart::Form::new()
            .text("chat_id", chat_id.to_owned())
            .part("document", part);
        if !caption.is_empty() {
            form = form.text("caption", caption.to_owned());
        }
        let response = self
            .client
            .post(self.url("sendDocument"))
            .multipart(form)
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        let body: Value = response
            .json()
            .await
            .map_err(|e| GatewayError::Parse(e.to_string()))?;
        if body.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(GatewayError::Transport(format!(
                "sendDocument rejected: {body}"
            )));
        }
        Ok(())
    }
}
