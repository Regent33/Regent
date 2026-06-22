//! `voice.*` JSON-RPC handlers — the speech subsystem's status surface. Read
//! the `SpeechConfig` snapshot + resolve provider availability from the
//! environment; the heavy operations (`voice.ensure_models`, `voice.test`) land
//! with the model manager + reqwest executor wiring.

use super::Dispatcher;
use crate::application::speech_factory;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use crate::infra::config_loader::expand_tilde;
use regent_kernel::{AudioFormat, TtsOptions};
use regent_speech::ModelManager;
use serde_json::json;

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
            speech_factory::provider_available(&s.asr.provider, &s.asr.base_url),
            speech_factory::provider_available(&s.tts.provider, &s.tts.base_url),
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

    /// `voice.test` — synthesize a short phrase through the configured TTS
    /// backend to prove the stack end-to-end. Requires a wired executor + a
    /// supported remote provider + a key; reports a clear error otherwise. The
    /// blocking synth runs off the runtime worker via `spawn_blocking`.
    pub(super) async fn voice_test(&self, req: RpcRequest) {
        let Some(cfg) = &self.config else {
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
        let synth = tokio::task::spawn_blocking(move || tts.synthesize("Regent voice test.", &opts))
            .await;
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
            Err(e) => self.send(err_response(req.id, -32000, format!("voice.test task: {e}"))),
        }
    }

    /// `voice.ensure_models` — download the configured local weight files into
    /// `models_dir`, idempotent and checksum-verified via the model manager. Run
    /// by `regent voice setup`/`enable`, so weights are fetched **only when voice
    /// is turned on** — never on a fresh, disabled install. No `weights`
    /// configured ⇒ nothing to download (a hosted provider, or a localhost server
    /// you run yourself). The blocking download runs off the runtime worker.
    pub(super) async fn voice_ensure_models(&self, req: RpcRequest) {
        let Some(cfg) = &self.config else {
            self.send(err_response(req.id, -32000, "config not wired"));
            return;
        };
        let specs = speech_factory::weight_specs(&cfg.speech);
        if specs.is_empty() {
            self.send(ok_response(
                req.id,
                json!({
                    "downloaded": [],
                    "note": "no weights configured to download (hosted provider or localhost server)",
                }),
            ));
            return;
        }
        let root = expand_tilde(&cfg.speech.models_dir);
        let handle = tokio::runtime::Handle::current();
        let done = tokio::task::spawn_blocking(move || {
            let mgr = ModelManager::new(root);
            let mut done = Vec::new();
            for spec in specs {
                let label = format!("{}/{}", spec.kind.dir(), spec.id);
                mgr.ensure(&spec, |url| {
                    if !weight_url_allowed(url) {
                        return Err(format!("refusing weight URL (HTTPS required): {url}"));
                    }
                    handle.block_on(async {
                        let resp = reqwest::get(url).await.map_err(|e| e.to_string())?;
                        if !resp.status().is_success() {
                            return Err(format!("HTTP {} for {url}", resp.status()));
                        }
                        // Bound memory + reject a hostile content-length up front.
                        if let Some(len) = resp.content_length()
                            && len > MAX_WEIGHT_BYTES
                        {
                            return Err(format!("weight exceeds {MAX_WEIGHT_BYTES}-byte cap: {len}"));
                        }
                        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
                        if bytes.len() as u64 > MAX_WEIGHT_BYTES {
                            return Err(format!("weight exceeds {MAX_WEIGHT_BYTES}-byte cap"));
                        }
                        Ok(bytes.to_vec())
                    })
                })
                .map_err(|e| e.to_string())?;
                done.push(label);
            }
            Ok::<Vec<String>, String>(done)
        })
        .await;
        match done {
            Ok(Ok(done)) => self.send(ok_response(req.id, json!({ "downloaded": done }))),
            Ok(Err(e)) => self.send(err_response(req.id, -32000, e)),
            Err(e) => {
                self.send(err_response(req.id, -32000, format!("voice.ensure_models task: {e}")))
            }
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

/// Per-file download cap (8 GiB) — bounds memory and a hostile `content-length`.
/// Generous for any Qwen3-1.7B variant.
const MAX_WEIGHT_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// A weight URL must be HTTPS — the file is later executed by the speech
/// runtime, so a plaintext/MITM'd download is code execution. `http` is allowed
/// only for loopback (a local mirror). Rejects every other scheme (`file://`,
/// `ftp://`) and unparseable input.
fn weight_url_allowed(url: &str) -> bool {
    match reqwest::Url::parse(url) {
        Ok(u) => match u.scheme() {
            "https" => true,
            "http" => matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "::1")),
            _ => false,
        },
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::weight_url_allowed;

    #[test]
    fn weight_urls_require_https_except_loopback() {
        assert!(weight_url_allowed("https://huggingface.co/o/r/resolve/main/m.bin"));
        assert!(weight_url_allowed("http://localhost:8000/m.bin"));
        assert!(weight_url_allowed("http://127.0.0.1/m.bin"));
        assert!(!weight_url_allowed("http://evil.example/m.bin")); // plaintext, non-loopback
        assert!(!weight_url_allowed("file:///etc/passwd")); // scheme not allowed
        assert!(!weight_url_allowed("ftp://x/m.bin"));
        assert!(!weight_url_allowed("not a url"));
    }
}
