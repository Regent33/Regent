//! Catalog invariants. The bug class these guard against: a menu that
//! advertises a backend which errors the moment it is selected — the previous
//! `BUILTIN_TTS_PROVIDERS` listed 11 names of which 4 resolved.

use super::*;

// Every row must be selectable end to end: an id the resolver can find, a base
// URL the adapter can build a request against, and at least one capability.
#[test]
fn every_row_is_usable_and_unique() {
    let mut seen = std::collections::HashSet::new();
    for p in SPEECH_PROVIDERS {
        assert!(seen.insert(p.id), "duplicate provider id: {}", p.id);
        assert_eq!(p.id, p.id.to_lowercase(), "{} must be lowercase", p.id);
        assert!(
            p.base_url.is_empty()
                || p.base_url.starts_with("https://")
                || p.base_url.starts_with("http://localhost"),
            "{}: remote endpoints must be HTTPS; only localhost may be plain HTTP",
            p.id
        );
        assert!(
            p.asr_model.is_some() || p.tts_model.is_some(),
            "{} declares neither ASR nor TTS",
            p.id
        );
        assert!(!p.label.is_empty() && !p.blurb.is_empty(), "{}", p.id);
        // A hosted API always needs a key. (A server you run may still declare
        // one — a LiteLLM proxy can be behind a master key — so this is an
        // implication, not an equivalence.)
        assert!(
            !p.hosted || p.key_var.is_some(),
            "{} is hosted but names no key var",
            p.id
        );
        // A row with no default host must say so in its blurb, since picking it
        // without setting base_url is an error rather than a working default.
        assert!(
            !p.base_url.is_empty() || p.blurb.contains("base_url"),
            "{} has no default base_url and its blurb never says to set one",
            p.id
        );
        assert!(find(p.id).is_some(), "{} is not resolvable", p.id);
    }
}

#[test]
fn lookup_normalizes_and_aliases_dashscope() {
    assert_eq!(find("  GROQ ").map(|p| p.id), Some("groq"));
    assert_eq!(find("dashscope").map(|p| p.id), Some("qwen"));
    assert_eq!(find("DashScope").map(|p| p.id), Some("qwen"));
    assert!(
        find("elevenlabs").is_none(),
        "not OpenAI-compatible; must not be listed"
    );
    assert!(find("").is_none());
}

// The delivered floor. Below this the catalog has silently regressed.
#[test]
fn at_least_twelve_of_each_kind() {
    assert!(asr_ids().len() >= 12, "ASR providers: {:?}", asr_ids());
    assert!(tts_ids().len() >= 12, "TTS providers: {:?}", tts_ids());
    // Kind lists must not leak a provider that lacks that capability.
    assert!(!asr_ids().contains(&"kokoro"), "kokoro is TTS-only");
    assert!(!tts_ids().contains(&"mistral"), "voxtral is ASR-only");
}
