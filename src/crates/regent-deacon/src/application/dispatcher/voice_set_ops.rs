//! `voice.set` — let the agent change the speech + vision models itself.

use super::Dispatcher;
use super::speech_yaml::set_config_speech_field;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use serde_json::json;

impl Dispatcher {
    /// `voice.set` — params (at least one): `asr_model`/`tts_model` rewrite
    /// `speech.<kind>.model` and `asr_provider`/`tts_provider` rewrite
    /// `speech.<kind>.provider` in config.yaml (parsed + re-serialized, same as
    /// `regent voice setup`); `whisper_size` (tiny|base|small|medium|…) sets
    /// `REGENT_WHISPER_SIZE` in `$REGENT_HOME/.env` — the live-call server's
    /// local ASR size; `vision_model`/`vision_base_url` set
    /// `REGENT_VISION_MODEL`/`REGENT_VISION_BASE_URL` in `.env` (what
    /// `vision_analyze` reads; the key stays in manage_keys). Nothing is
    /// hot-swapped: changes apply on the next deacon/voice-server start, and
    /// the reply says so.
    pub(super) fn voice_set(&self, req: RpcRequest) {
        let get = |key: &str| {
            req.params
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
        };
        let (asr, tts, size) = (get("asr_model"), get("tts_model"), get("whisper_size"));
        let (asr_provider, tts_provider) = (get("asr_provider"), get("tts_provider"));
        let env_sets: Vec<(&str, Option<String>)> = vec![
            ("REGENT_VISION_MODEL", get("vision_model")),
            ("REGENT_VISION_BASE_URL", get("vision_base_url")),
        ];
        if asr.is_none()
            && tts.is_none()
            && asr_provider.is_none()
            && tts_provider.is_none()
            && size.is_none()
            && env_sets.iter().all(|(_, v)| v.is_none())
        {
            self.send(err_response(
                req.id,
                -32602,
                "give at least one of: asr_model, tts_model, asr_provider, tts_provider, whisper_size, vision_model, vision_base_url",
            ));
            return;
        }
        if let Some(s) = &size
            && !valid_whisper_size(s)
        {
            self.send(err_response(
                req.id,
                -32602,
                format!("whisper_size '{s}' — use a sherpa-onnx whisper release name (tiny|base|small|medium|…)"),
            ));
            return;
        }
        let Ok(home) = std::env::var("REGENT_HOME") else {
            self.send(err_response(req.id, -32000, "REGENT_HOME is not set"));
            return;
        };
        let mut changed = Vec::new();
        if asr.is_some() || tts.is_some() {
            match set_config_speech_field(
                std::path::Path::new(&home),
                "model",
                asr.as_deref(),
                tts.as_deref(),
            ) {
                Ok(mut c) => changed.append(&mut c),
                Err(e) => {
                    self.send(err_response(req.id, -32000, e));
                    return;
                }
            }
        }
        if asr_provider.is_some() || tts_provider.is_some() {
            match set_config_speech_field(
                std::path::Path::new(&home),
                "provider",
                asr_provider.as_deref(),
                tts_provider.as_deref(),
            ) {
                Ok(mut c) => changed.append(&mut c),
                Err(e) => {
                    self.send(err_response(req.id, -32000, e));
                    return;
                }
            }
        }
        if let Some(s) = &size {
            if let Err(e) = regent_tools::upsert_env_var("REGENT_WHISPER_SIZE", s) {
                self.send(err_response(req.id, -32000, e));
                return;
            }
            changed.push(format!("REGENT_WHISPER_SIZE={s} (.env)"));
        }
        for (key, value) in env_sets {
            let Some(value) = value else { continue };
            if let Err(e) = regent_tools::upsert_env_var(key, &value) {
                self.send(err_response(req.id, -32000, e));
                return;
            }
            changed.push(format!("{key}={value} (.env)"));
        }
        self.send(ok_response(
            req.id,
            json!({
                "changed": changed,
                "note": "saved; applies on the next deacon/voice-server start (e.g. the next `regent call`), not this session",
            }),
        ));
    }
}

/// A whisper size becomes a download URL segment + a directory name, so it
/// must stay a plain release-name token (e.g. `small`, `medium.en`, `large-v3`).
fn valid_whisper_size(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 32
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

#[cfg(test)]
mod tests {
    use super::valid_whisper_size;

    #[test]
    fn whisper_size_is_a_plain_release_token() {
        for ok in ["small", "medium.en", "large-v3", "tiny_int8"] {
            assert!(valid_whisper_size(ok), "{ok}");
        }
        for bad in ["", "a/b", "x y", "..\\up", &"x".repeat(33)] {
            assert!(!valid_whisper_size(bad), "{bad}");
        }
    }
}
