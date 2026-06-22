//! Telegram voice (turn-based): download + transcribe inbound notes, speak
//! replies in chats that spoke. Beside the core adapter so `telegram.rs` stays
//! focused on poll/text; `TelegramAdapter`'s private fields are reachable here
//! (descendant module). Mirrors Hermes's auto-TTS-on-voice-input.

use super::TelegramAdapter;
use crate::domain::entities::OutboundMessage;
use crate::domain::errors::GatewayError;
use regent_kernel::{AsrOptions, AudioFormat, TtsOptions};
use serde_json::{Value, json};

/// Telegram's public Bot API caps `getFile` downloads at 20 MB.
const MAX_FILE_BYTES: i64 = 20 * 1024 * 1024;

impl TelegramAdapter {
    pub(super) fn mark_voice(&self, chat_id: &str) {
        self.voice_chats
            .lock()
            .expect("voice_chats poisoned")
            .insert(chat_id.to_owned());
    }

    pub(super) fn clear_voice(&self, chat_id: &str) {
        self.voice_chats
            .lock()
            .expect("voice_chats poisoned")
            .remove(chat_id);
    }

    pub(super) fn is_voice_chat(&self, chat_id: &str) -> bool {
        self.voice_chats
            .lock()
            .expect("voice_chats poisoned")
            .contains(chat_id)
    }

    /// `getFile` → download the bytes (rejecting anything over the 20 MB cap
    /// before fetching).
    async fn download_file(&self, file_id: &str) -> Result<Vec<u8>, GatewayError> {
        let info = self.call("getFile", json!({ "file_id": file_id })).await?;
        if let Some(size) = info.pointer("/result/file_size").and_then(Value::as_i64)
            && size > MAX_FILE_BYTES
        {
            return Err(GatewayError::Transport(format!(
                "voice file too large ({size} bytes; 20 MB Bot API cap)"
            )));
        }
        let file_path = info
            .pointer("/result/file_path")
            .and_then(Value::as_str)
            .ok_or_else(|| GatewayError::Parse("getFile: missing file_path".to_owned()))?;
        let url = format!(
            "https://api.telegram.org/file/bot{}/{}",
            self.token, file_path
        );
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        Ok(bytes.to_vec())
    }

    /// Download + transcribe a voice note to text. Never errors out the poll
    /// loop — failures become a short note so the user still gets a reply.
    pub(super) async fn transcribe_voice(&self, file_id: &str) -> String {
        let Some(asr) = self.asr.clone() else {
            return "(voice message received, but voice isn't enabled — run `regent voice setup`)"
                .to_owned();
        };
        let bytes = match self.download_file(file_id).await {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::warn!(%error, "voice download failed");
                return "(couldn't download the voice message)".to_owned();
            }
        };
        let result = tokio::task::spawn_blocking(move || {
            asr.transcribe_file(&bytes, "voice.ogg", &AsrOptions::default())
        })
        .await;
        match result {
            Ok(Ok(t)) if !t.text.trim().is_empty() => t.text,
            Ok(Ok(_)) => "(the voice message was empty or unclear)".to_owned(),
            Ok(Err(error)) => {
                tracing::warn!(%error, "voice transcription failed");
                "(voice transcription failed)".to_owned()
            }
            Err(error) => {
                tracing::warn!(%error, "transcription task failed");
                "(voice transcription failed)".to_owned()
            }
        }
    }

    /// Synthesize `message.text` to Opus and deliver it as a `sendVoice` bubble.
    pub(super) async fn send_voice_reply(
        &self,
        message: &OutboundMessage,
    ) -> Result<(), GatewayError> {
        let tts = self
            .tts
            .clone()
            .ok_or_else(|| GatewayError::Transport("tts not configured".to_owned()))?;
        let text = message.text.clone();
        let opts = TtsOptions {
            format: AudioFormat::Opus,
            ..TtsOptions::default()
        };
        let synth = tokio::task::spawn_blocking(move || tts.synthesize(&text, &opts))
            .await
            .map_err(|e| GatewayError::Transport(format!("tts task: {e}")))?
            .map_err(|e| GatewayError::Transport(format!("tts: {e}")))?;
        let part = reqwest::multipart::Part::bytes(synth.bytes)
            .file_name("reply.ogg")
            .mime_str("audio/ogg")
            .map_err(|e| GatewayError::Transport(e.to_string()))?;
        let form = reqwest::multipart::Form::new()
            .text("chat_id", message.chat_id.clone())
            .part("voice", part);
        let body: Value = self
            .client
            .post(self.url("sendVoice"))
            .multipart(form)
            .send()
            .await
            .map_err(|e| GatewayError::Transport(e.to_string()))?
            .json()
            .await
            .map_err(|e| GatewayError::Parse(e.to_string()))?;
        if body.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(GatewayError::Transport(format!("sendVoice rejected: {body}")));
        }
        Ok(())
    }
}

/// Extracts `(chat_id, user_id, file_id)` for each inbound voice note or audio
/// file. Pure — the download/transcribe happens in `next_event`.
#[must_use]
pub(super) fn parse_voice(body: &Value) -> Vec<(String, String, String)> {
    let Some(updates) = body.get("result").and_then(Value::as_array) else {
        return Vec::new();
    };
    updates
        .iter()
        .filter_map(|update| {
            let message = update.get("message")?;
            let file_id = message
                .pointer("/voice/file_id")
                .or_else(|| message.pointer("/audio/file_id"))
                .and_then(Value::as_str)?;
            Some((
                message.pointer("/chat/id")?.as_i64()?.to_string(),
                message.pointer("/from/id")?.as_i64()?.to_string(),
                file_id.to_owned(),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_voice_and_audio_file_ids_and_skips_text() {
        let body = json!({"ok": true, "result": [
            {"update_id": 10, "message": {
                "voice": {"file_id": "VID", "file_size": 1000}, "chat": {"id": 7}, "from": {"id": 3}}},
            {"update_id": 11, "message": {
                "audio": {"file_id": "AID"}, "chat": {"id": 8}, "from": {"id": 4}}},
            {"update_id": 12, "message": {"text": "hi", "chat": {"id": 1}, "from": {"id": 2}}}
        ]});
        let voices = parse_voice(&body);
        assert_eq!(voices.len(), 2);
        assert_eq!(voices[0], ("7".to_owned(), "3".to_owned(), "VID".to_owned()));
        assert_eq!(voices[1], ("8".to_owned(), "4".to_owned(), "AID".to_owned()));
        assert!(parse_voice(&json!({"result": [
            {"update_id": 1, "message": {"text": "x", "chat": {"id": 1}, "from": {"id": 2}}}
        ]}))
        .is_empty());
    }

    #[test]
    fn voice_mode_tracks_per_chat() {
        let a = TelegramAdapter::new("t");
        assert!(!a.is_voice_chat("7"));
        a.mark_voice("7");
        assert!(a.is_voice_chat("7"));
        a.clear_voice("7");
        assert!(!a.is_voice_chat("7"));
        assert!(a.tts.is_none()); // no speech wired
    }
}
