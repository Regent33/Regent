//! Unit tests for `registry` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use regent_kernel::{RegentError, SynthesizedAudio, TtsOptions, TtsProvider};

/// Minimal real provider — proves the registry works with an actual trait
/// object (`Arc<dyn TtsProvider>`), not just a placeholder type.
struct Dummy(&'static str);
impl TtsProvider for Dummy {
    fn name(&self) -> &str {
        self.0
    }
    fn synthesize(&self, _text: &str, opts: &TtsOptions) -> Result<SynthesizedAudio, RegentError> {
        Ok(SynthesizedAudio {
            bytes: Vec::new(),
            format: opts.format,
        })
    }
}

fn tts_registry() -> ProviderRegistry<dyn TtsProvider> {
    ProviderRegistry::new("TTS", BUILTIN_TTS_PROVIDERS)
}

fn dummy(name: &'static str) -> Arc<dyn TtsProvider> {
    Arc::new(Dummy(name))
}

#[test]
fn registers_and_looks_up_case_and_whitespace_insensitively() {
    let reg = tts_registry();
    reg.register("Cartesia", dummy("cartesia")).unwrap();
    assert!(reg.get("  cartesia ").is_some());
    assert!(reg.get("CARTESIA").is_some());
    assert!(reg.get("missing").is_none());
    assert_eq!(reg.names(), vec!["cartesia".to_string()]);
}

#[test]
fn built_in_names_cannot_be_shadowed() {
    let reg = tts_registry();
    assert!(reg.is_builtin("openai"));
    assert!(reg.is_builtin("  Edge "));
    let err = reg.register("openai", dummy("openai")).unwrap_err();
    assert_eq!(err, RegistryError::ShadowsBuiltin("openai".into()));
    // Nothing was stored — a built-in name never resolves to a plugin.
    assert!(reg.get("openai").is_none());
    assert!(reg.names().is_empty());
}

#[test]
fn empty_or_whitespace_name_is_rejected() {
    let reg = tts_registry();
    assert_eq!(
        reg.register("   ", dummy("x")).unwrap_err(),
        RegistryError::EmptyName
    );
}

#[test]
fn re_registration_overwrites() {
    let reg = tts_registry();
    reg.register("voicebox", dummy("first")).unwrap();
    reg.register("voicebox", dummy("second")).unwrap();
    assert_eq!(reg.get("voicebox").unwrap().name(), "second");
    assert_eq!(reg.names().len(), 1);
}

#[test]
fn asr_and_tts_reserve_different_built_in_sets() {
    let asr: ProviderRegistry<dyn TtsProvider> =
        ProviderRegistry::new("ASR", BUILTIN_ASR_PROVIDERS);
    // `groq`/`local_command` are ASR built-ins; `piper`/`edge` are TTS-only.
    assert!(asr.is_builtin("groq"));
    assert!(asr.is_builtin("local_command"));
    assert!(!asr.is_builtin("piper"));
    assert!(!asr.is_builtin("edge"));
}
