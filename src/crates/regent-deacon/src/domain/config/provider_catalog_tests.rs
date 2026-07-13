//! Unit tests for `provider_catalog` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::ProviderKind;

#[test]
fn anthropic_lists_the_curated_ids_and_no_kind_panics() {
    // Exhaustive: every variant returns a (possibly empty) &'static slice.
    for kind in [
        ProviderKind::Anthropic,
        ProviderKind::Openai,
        ProviderKind::OpenRouter,
        ProviderKind::Groq,
        ProviderKind::DeepSeek,
        ProviderKind::Together,
        ProviderKind::Ollama,
        ProviderKind::Mistral,
        ProviderKind::Xai,
        ProviderKind::Gemini,
        ProviderKind::Moonshot,
        ProviderKind::Zhipu,
        ProviderKind::DashScope,
        ProviderKind::Fireworks,
        ProviderKind::Cerebras,
        ProviderKind::Perplexity,
        ProviderKind::Minimax,
        ProviderKind::Nvidia,
    ] {
        let _ = kind.default_models();
    }
    let anthropic = ProviderKind::Anthropic.default_models();
    assert!(anthropic.contains(&"claude-opus-4-8"));
    assert!(anthropic.contains(&"claude-fable-5"));
    // Ollama is intentionally empty (local, machine-specific).
    assert!(ProviderKind::Ollama.default_models().is_empty());
}

#[test]
fn every_remote_kind_offers_a_usable_catalog() {
    // The user-facing bar: every remote kind lists at least 5 pickable
    // models (DeepSeek's native API genuinely serves only 2 rolling
    // aliases), and OpenRouter carries the wide multi-org catalog.
    assert!(ProviderKind::OpenRouter.default_models().len() >= 60);
    for kind in [
        ProviderKind::Anthropic,
        ProviderKind::Openai,
        ProviderKind::Groq,
        ProviderKind::Together,
        ProviderKind::Mistral,
        ProviderKind::Xai,
        ProviderKind::Gemini,
        ProviderKind::Moonshot,
        ProviderKind::Zhipu,
        ProviderKind::DashScope,
        ProviderKind::Fireworks,
        ProviderKind::Cerebras,
        ProviderKind::Perplexity,
        ProviderKind::Nvidia,
    ] {
        assert!(kind.default_models().len() >= 5, "{kind:?} lists too few");
    }
}
