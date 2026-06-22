//! `voice.*` JSON-RPC handlers — the speech subsystem's status surface. Read
//! the `SpeechConfig` snapshot + resolve provider availability from the
//! environment; the heavy operations (`voice.ensure_models`, `voice.test`) land
//! with the model manager + reqwest executor wiring.

use super::Dispatcher;
use crate::application::speech_factory;
use crate::domain::entities::{RpcRequest, err_response, ok_response};

impl Dispatcher {
    /// `voice.status` — enabled flag, configured ASR/TTS provider+model, and
    /// whether each is usable right now (supported remote backend + key set).
    pub(super) fn voice_status(&self, req: RpcRequest) {
        let Some(cfg) = &self.config else {
            self.send(err_response(req.id, -32000, "config not wired"));
            return;
        };
        let s = &cfg.speech;
        let payload = speech_factory::voice_status(
            s,
            speech_factory::provider_available(&s.asr.provider),
            speech_factory::provider_available(&s.tts.provider),
        );
        self.send(ok_response(req.id, payload));
    }

    /// `voice.models` — configured providers/models + the built-in names to pick.
    pub(super) fn voice_models(&self, req: RpcRequest) {
        let Some(cfg) = &self.config else {
            self.send(err_response(req.id, -32000, "config not wired"));
            return;
        };
        self.send(ok_response(req.id, speech_factory::voice_models(&cfg.speech)));
    }
}
