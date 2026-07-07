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

    /// `voice.set` — let the agent change the speech + vision models itself.
    /// Params (at least one): `asr_model`/`tts_model` rewrite
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
                            return Err(format!(
                                "weight exceeds {MAX_WEIGHT_BYTES}-byte cap: {len}"
                            ));
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
            Err(e) => self.send(err_response(
                req.id,
                -32000,
                format!("voice.ensure_models task: {e}"),
            )),
        }
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

/// Surgical config.yaml edit: set one field (`model` or `provider`) under
/// `speech.asr` / `speech.tts`, leaving every other key as parsed. Returns
/// "what changed" labels.
fn set_config_speech_field(
    home: &std::path::Path,
    field: &str,
    asr: Option<&str>,
    tts: Option<&str>,
) -> Result<Vec<String>, String> {
    let path = home.join("config.yaml");
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut doc: serde_yaml::Value =
        serde_yaml::from_str(&raw).map_err(|e| format!("config.yaml: {e}"))?;
    let mut changed = Vec::new();
    for (kind, value) in [("asr", asr), ("tts", tts)] {
        let Some(value) = value else { continue };
        let speech = ensure_map(&mut doc, "speech")?;
        let section = ensure_map(speech, kind)?;
        section
            .as_mapping_mut()
            .unwrap()
            .insert(field.into(), value.into());
        changed.push(format!("speech.{kind}.{field}={value} (config.yaml)"));
    }
    let out = serde_yaml::to_string(&doc).map_err(|e| e.to_string())?;
    std::fs::write(&path, out).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(changed)
}

/// Get `key` as a mapping inside `doc`, creating/replacing as needed.
fn ensure_map<'a>(
    doc: &'a mut serde_yaml::Value,
    key: &str,
) -> Result<&'a mut serde_yaml::Value, String> {
    let map = doc
        .as_mapping_mut()
        .ok_or_else(|| "config.yaml is not a mapping".to_owned())?;
    let k = serde_yaml::Value::from(key);
    if !map.get(&k).is_some_and(serde_yaml::Value::is_mapping) {
        map.insert(k.clone(), serde_yaml::Value::Mapping(Default::default()));
    }
    Ok(map.get_mut(&k).unwrap())
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
    use super::{set_config_speech_field, valid_whisper_size, weight_url_allowed};

    #[test]
    fn whisper_size_is_a_plain_release_token() {
        for ok in ["small", "medium.en", "large-v3", "tiny_int8"] {
            assert!(valid_whisper_size(ok), "{ok}");
        }
        for bad in ["", "a/b", "x y", "..\\up", &"x".repeat(33)] {
            assert!(!valid_whisper_size(bad), "{bad}");
        }
    }

    #[test]
    fn set_config_models_edits_only_the_model_keys() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("config.yaml"),
            "_config_version: 1\nmodel:\n  default: minimax-m3\nspeech:\n  enabled: true\n  asr:\n    provider: local\n    model: old-asr\n",
        )
        .unwrap();
        let changed =
            set_config_speech_field(dir.path(), "model", Some("new-asr"), Some("new-tts")).unwrap();
        assert_eq!(changed.len(), 2);
        let doc: serde_yaml::Value =
            serde_yaml::from_str(&std::fs::read_to_string(dir.path().join("config.yaml")).unwrap())
                .unwrap();
        assert_eq!(doc["speech"]["asr"]["model"], "new-asr");
        assert_eq!(doc["speech"]["asr"]["provider"], "local", "sibling kept");
        assert_eq!(doc["speech"]["tts"]["model"], "new-tts", "section created");
        assert_eq!(doc["speech"]["enabled"], true);
        assert_eq!(doc["model"]["default"], "minimax-m3", "other sections kept");
    }

    #[test]
    fn set_config_speech_field_edits_only_the_provider_keys() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("config.yaml"),
            "_config_version: 1\nmodel:\n  default: minimax-m3\nspeech:\n  enabled: true\n  asr:\n    provider: local\n    model: old-asr\n",
        )
        .unwrap();
        let changed =
            set_config_speech_field(dir.path(), "provider", Some("openai"), Some("elevenlabs"))
                .unwrap();
        assert_eq!(changed.len(), 2);
        let doc: serde_yaml::Value =
            serde_yaml::from_str(&std::fs::read_to_string(dir.path().join("config.yaml")).unwrap())
                .unwrap();
        assert_eq!(doc["speech"]["asr"]["provider"], "openai");
        assert_eq!(
            doc["speech"]["asr"]["model"], "old-asr",
            "sibling model kept"
        );
        assert_eq!(
            doc["speech"]["tts"]["provider"], "elevenlabs",
            "section created"
        );
        assert_eq!(doc["speech"]["enabled"], true);
        assert_eq!(doc["model"]["default"], "minimax-m3", "other sections kept");
    }

    #[test]
    fn weight_urls_require_https_except_loopback() {
        assert!(weight_url_allowed(
            "https://huggingface.co/o/r/resolve/main/m.bin"
        ));
        assert!(weight_url_allowed("http://localhost:8000/m.bin"));
        assert!(weight_url_allowed("http://127.0.0.1/m.bin"));
        assert!(!weight_url_allowed("http://evil.example/m.bin")); // plaintext, non-loopback
        assert!(!weight_url_allowed("file:///etc/passwd")); // scheme not allowed
        assert!(!weight_url_allowed("ftp://x/m.bin"));
        assert!(!weight_url_allowed("not a url"));
    }
}
