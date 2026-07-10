//! `voice.*` JSON-RPC read/test handlers — the speech subsystem's status
//! surface. `voice.set` lives in `voice_set_ops`, model downloads in
//! `voice_weights_ops`.

use super::Dispatcher;
use crate::application::speech_factory;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use regent_kernel::{AudioFormat, TtsOptions};
use serde_json::json;

impl Dispatcher {
    /// `voice.status` — enabled flag, configured ASR/TTS provider+model, and
    /// whether each is usable right now (supported remote backend + key set).
    pub(super) fn voice_status(&self, req: RpcRequest) {
        let Some(cfg) = self.config_snapshot() else {
            self.send(err_response(req.id, -32000, "config not wired"));
            return;
        };
        let s = &cfg.speech;
        let payload = speech_factory::voice_status(
            s,
            speech_factory::provider_available(&s.asr.provider, &s.asr.base_url),
            speech_factory::provider_available(&s.tts.provider, &s.tts.base_url),
        );
        self.send(ok_response(req.id, payload));
    }

    /// `voice.models` — configured providers/models + the built-in names to pick.
    pub(super) fn voice_models(&self, req: RpcRequest) {
        let Some(cfg) = self.config_snapshot() else {
            self.send(err_response(req.id, -32000, "config not wired"));
            return;
        };
        self.send(ok_response(
            req.id,
            speech_factory::voice_models(&cfg.speech),
        ));
    }

    /// `voice.test` — synthesize a short phrase through the configured TTS
    /// backend to prove the stack end-to-end. Requires a wired executor + a
    /// supported remote provider + a key; reports a clear error otherwise. The
    /// blocking synth runs off the runtime worker via `spawn_blocking`.
    pub(super) async fn voice_test(&self, req: RpcRequest) {
        let Some(cfg) = self.config_snapshot() else {
            self.send(err_response(req.id, -32000, "config not wired"));
            return;
        };
        let Some(exec) = &self.speech_exec else {
            self.send(err_response(req.id, -32000, "speech executor not wired"));
            return;
        };
        let speech = cfg.speech.clone();
        let tts = match speech_factory::make_tts(&speech, std::sync::Arc::clone(exec)) {
            Ok(tts) => tts,
            Err(e) => {
                self.send(err_response(req.id, -32000, e));
                return;
            }
        };
        let opts = TtsOptions {
            voice: (speech.tts.voice != "default").then_some(speech.tts.voice.clone()),
            model: None,
            speed: None,
            format: parse_format(&speech.tts.format),
        };
        let synth =
            tokio::task::spawn_blocking(move || tts.synthesize("Regent voice test.", &opts)).await;
        match synth {
            Ok(Ok(audio)) => self.send(ok_response(
                req.id,
                json!({
                    "ok": true,
                    "provider": speech.tts.provider,
                    "bytes": audio.bytes.len(),
                    "format": audio.format.ext(),
                }),
            )),
            Ok(Err(e)) => self.send(err_response(req.id, -32000, e.to_string())),
            Err(e) => self.send(err_response(
                req.id,
                -32000,
                format!("voice.test task: {e}"),
            )),
        }
    }
}

/// Map a config format string to [`AudioFormat`], defaulting to Mp3.
fn parse_format(s: &str) -> AudioFormat {
    match s.trim().to_lowercase().as_str() {
        "opus" => AudioFormat::Opus,
        "wav" => AudioFormat::Wav,
        "ogg" => AudioFormat::Ogg,
        "flac" => AudioFormat::Flac,
        _ => AudioFormat::Mp3,
    }
}
