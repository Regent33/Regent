//! Unit tests for `speech_factory` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use regent_speech::SpeechHttpRequest;

#[test]
fn weight_specs_empty_by_default_and_built_from_config() {
    assert!(weight_specs(&SpeechConfig::default()).is_empty());

    let mut cfg = SpeechConfig::default();
    cfg.asr.weights = vec![WeightFile {
        name: "model.bin".into(),
        url: "https://example/model.bin".into(),
        sha256: String::new(),
    }];
    let specs = weight_specs(&cfg);
    assert_eq!(specs.len(), 1); // tts still empty → no spec
    assert_eq!(specs[0].kind, ModelKind::Asr);
    assert_eq!(specs[0].id, "whisper");
    assert_eq!(specs[0].files[0].name, "model.bin");
}

struct NoopExecutor;
impl HttpExecutor for NoopExecutor {
    fn execute(&self, _req: SpeechHttpRequest) -> Result<Vec<u8>, String> {
        Ok(Vec::new())
    }
}

/// Captures the request the adapter built, so a test can assert on what would
/// go over the wire (URL, model) without any network.
#[derive(Default)]
struct Recorder {
    last: std::sync::Mutex<Option<SpeechHttpRequest>>,
}

impl Recorder {
    fn last_body(&self) -> Value {
        match self.last.lock().expect("recorder").clone() {
            Some(SpeechHttpRequest {
                body: regent_speech::HttpBody::Json(v),
                ..
            }) => v,
            other => panic!("expected a JSON body, got {other:?}"),
        }
    }
}

impl HttpExecutor for Recorder {
    fn execute(&self, req: SpeechHttpRequest) -> Result<Vec<u8>, String> {
        *self.last.lock().expect("recorder") = Some(req);
        Ok(Vec::new())
    }
}

#[test]
fn make_asr_builds_local_by_default_and_rejects_unknown() {
    // Default provider is `local` — it builds without a key.
    let cfg = SpeechConfig::default();
    let asr = make_asr(&cfg, Arc::new(NoopExecutor)).expect("local builds");
    assert_eq!(asr.name(), "local");

    // An unknown id is refused, and the error lists what to pick instead
    // (wording changed from "not wired" now that the catalog names the options).
    let mut bad = SpeechConfig::default();
    bad.asr.provider = "nope".into();
    let err = make_asr(&bad, Arc::new(NoopExecutor))
        .err()
        .expect("rejected");
    assert!(err.contains("not known"), "{err}");
    assert!(
        err.contains("groq"),
        "the error must name real choices: {err}"
    );
}

// The escape hatch: an id the catalog has never heard of still builds when the
// user supplies a base URL, because the adapter only needs an endpoint. This is
// how an unlisted or brand-new OpenAI-compatible host works without a release.
#[test]
fn an_unknown_provider_with_a_base_url_still_builds() {
    let mut cfg = SpeechConfig::default();
    cfg.asr.provider = "my-own-box".into();
    cfg.asr.base_url = "https://speech.example.com/v1".into();
    let asr = make_asr(&cfg, Arc::new(NoopExecutor)).expect("base_url is enough");
    assert_eq!(asr.name(), "my-own-box");
}

// Picking a speak-only backend for transcription used to build an adapter that
// could only 404 at call time; it now fails fast and names the ASR options.
#[test]
fn a_tts_only_provider_is_refused_for_asr() {
    let mut cfg = SpeechConfig::default();
    cfg.asr.provider = "kokoro".into();
    let err = make_asr(&cfg, Arc::new(NoopExecutor))
        .err()
        .expect("rejected");
    assert!(err.contains("does not do speech-to-text"), "{err}");
}

// Switching provider without also editing the model id must not send an empty
// model — the catalog's default for that endpoint fills in.
#[test]
fn an_empty_model_falls_back_to_the_catalog_default() {
    let cfg = SpeechConfig {
        tts: crate::domain::config::TtsConfig {
            provider: "deepinfra".into(),
            model: String::new(),
            ..Default::default()
        },
        ..SpeechConfig::default()
    };
    // Proven through the built request: the adapter carries the resolved model.
    let recorder = Arc::new(Recorder::default());
    let tts = make_tts(&cfg, Arc::clone(&recorder) as Arc<dyn HttpExecutor>).expect("builds");
    let _ = tts.synthesize("hi", &regent_kernel::TtsOptions::default());
    let body = recorder.last_body();
    assert_eq!(body["model"], "hexgrad/Kokoro-82M", "body: {body}");
}

#[test]
fn make_tts_accepts_a_dyn_executor() {
    let cfg = SpeechConfig {
        tts: crate::domain::config::TtsConfig {
            provider: "qwen".into(),
            ..Default::default()
        },
        ..SpeechConfig::default()
    };
    let exec: Arc<dyn HttpExecutor> = Arc::new(NoopExecutor);
    let tts = make_tts(&cfg, exec).expect("remote builds");
    assert_eq!(tts.name(), "qwen");
}

#[test]
fn resolve_base_defaults_local_to_localhost_and_honors_override() {
    assert!(resolve_base("local", "").unwrap().contains("localhost"));
    assert_eq!(
        resolve_base("groq", ""),
        Some("https://api.groq.com/openai/v1".into())
    );
    assert!(resolve_base("QWEN", "").is_some()); // case-insensitive
    assert_eq!(resolve_base("nope", ""), None);
    // An explicit override wins for any provider (e.g. a local server's URL).
    assert_eq!(
        resolve_base("local", "http://127.0.0.1:1234/v1"),
        Some("http://127.0.0.1:1234/v1".into())
    );
}

#[test]
fn local_needs_no_key_and_is_available_by_default() {
    assert!(!needs_key("local"));
    assert!(needs_key("groq"));
    // local is available once configured (no key required)…
    assert!(provider_available("local", ""));
    // …an unknown provider never is.
    assert!(!provider_available("nope", ""));
}

#[test]
fn status_payload_reflects_config_and_availability() {
    let cfg = SpeechConfig::default();
    let v = voice_status(&cfg, true, false);
    assert_eq!(v["enabled"], false);
    assert_eq!(v["asr"]["model"], "whisper");
    assert_eq!(v["asr"]["available"], true);
    assert_eq!(v["tts"]["model"], "kokoro-en-v0_19");
    assert_eq!(v["tts"]["available"], false);
    assert_eq!(v["call"]["fast_model"], "");
    // No REGENT_WHISPER_SIZE/REGENT_KOKORO_* in the test env ⇒ the
    // documented defaults.
    assert_eq!(v["whisper_size"], "small");
    assert_eq!(v["kokoro_speaker"], "0");
    assert_eq!(v["kokoro_speed"], "1");
}

#[test]
fn models_payload_lists_configured_and_builtins() {
    let v = voice_models(&SpeechConfig::default());
    assert_eq!(v["asr"]["configured"]["provider"], "local");
    let asr_builtins = v["asr"]["builtins"].as_array().unwrap();
    assert!(asr_builtins.iter().any(|p| p == "groq"));
    let tts_builtins = v["tts"]["builtins"].as_array().unwrap();
    assert!(tts_builtins.iter().any(|p| p == "edge"));
}
