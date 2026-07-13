//! Unit tests for `provider_kind` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::ProviderKind;

// Every variant Regent knows — the source of truth for the exhaustive tests.
const ALL: &[ProviderKind] = &[
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
];

#[test]
fn every_kind_has_a_key_var_and_an_https_endpoint_and_round_trips_via_serde() {
    for &kind in ALL {
        // A non-empty UPPER_SNAKE key var.
        let var = kind.key_env_var();
        assert!(var.ends_with("_API_KEY"), "{kind:?}: {var}");
        // A reachable-looking base + a chat-completions path.
        let (base, path) = kind.openai_base_path();
        assert!(base.starts_with("http"), "{kind:?}: {base}");
        assert!(path.ends_with("/chat/completions"), "{kind:?}: {path}");
        // serde lowercase name parses back to the same variant.
        let name = serde_json::to_string(&kind).unwrap();
        let name = name.trim_matches('"');
        assert_eq!(ProviderKind::parse(name), Some(kind), "{name}");
    }
}

#[test]
fn known_key_vars_are_stable() {
    assert_eq!(ProviderKind::Ollama.key_env_var(), "OLLAMA_API_KEY");
    assert_eq!(ProviderKind::OpenRouter.key_env_var(), "OPENROUTER_API_KEY");
    assert_eq!(ProviderKind::Gemini.key_env_var(), "GEMINI_API_KEY");
    assert_eq!(ProviderKind::Minimax.key_env_var(), "MINIMAX_API_KEY");
}
