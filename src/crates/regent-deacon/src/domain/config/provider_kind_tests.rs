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

#[test]
fn all_covers_every_kind_and_names_round_trip() {
    for kind in ProviderKind::ALL {
        assert_eq!(ProviderKind::parse(kind.name()), Some(kind), "{kind:?}");
        // `name()` must agree with the serde wire form `parse` documents.
        let wire = serde_json::to_string(&kind).unwrap();
        assert_eq!(format!("\"{}\"", kind.name()), wire);
    }
    // A new enum variant must be added to ALL — this count pins it.
    assert_eq!(ProviderKind::ALL.len(), 19);
}

#[test]
fn ollama_local_and_cloud_are_distinct_kinds() {
    // They differ by endpoint and key, not protocol — the same relationship
    // Openai/OpenRouter have. Collapsing them back into one kind (leaving the
    // hosted service reachable only by remembering to repoint base_url at
    // ollama.com) is the regression this catches.
    let (local, _) = ProviderKind::Ollama.openai_base_path();
    let (cloud, _) = ProviderKind::OllamaCloud.openai_base_path();
    assert_eq!(local, "http://localhost:11434");
    assert_eq!(cloud, "https://ollama.com");
    // The wire name has a hyphen; the lowercase serde default would silently
    // make it "ollamacloud" and every existing config reference would miss.
    assert_eq!(ProviderKind::OllamaCloud.name(), "ollama-cloud");
    assert_eq!(
        ProviderKind::parse("ollama-cloud"),
        Some(ProviderKind::OllamaCloud)
    );
    // Both bill to the same account, so they share a key var.
    assert_eq!(ProviderKind::OllamaCloud.key_env_var(), "OLLAMA_API_KEY");
    // The hosted catalog is real; the local one is whatever you've pulled.
    assert!(!ProviderKind::OllamaCloud.default_models().is_empty());
    assert!(ProviderKind::Ollama.default_models().is_empty());
}
