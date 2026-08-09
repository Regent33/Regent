//! Unit tests for `provider_kind` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::ProviderKind;

// This file used to keep its OWN hand-copied list of variants and iterate that.
// It had already drifted — no `OllamaCloud`, and it would have missed every kind
// added since — so the "exhaustive" test quietly stopped being exhaustive. Every
// test below now iterates `ProviderKind::ALL` itself; its assertions live on in
// `every_kind_yields_a_well_formed_endpoint_and_key_var`.

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
    assert_eq!(ProviderKind::ALL.len(), 33);
}

// Every kind must produce an endpoint that can actually be called: an absolute
// URL and a path that starts at the root. A typo'd base is otherwise only
// discovered by a user whose first request 404s.
#[test]
fn every_kind_yields_a_well_formed_endpoint_and_key_var() {
    for kind in ProviderKind::ALL {
        let (base, path) = kind.openai_base_path();
        let local = base.starts_with("http://localhost");
        assert!(
            base.starts_with("https://") || local,
            "{kind:?}: only localhost may be plain HTTP ({base})"
        );
        assert!(
            !base.ends_with('/'),
            "{kind:?}: base must not end in / ({base})"
        );
        assert!(
            path.starts_with('/'),
            "{kind:?}: path must be rooted ({path})"
        );
        assert!(
            path.ends_with("/chat/completions"),
            "{kind:?}: OpenAI-compatible chat path expected ({path})"
        );
        let var = kind.key_env_var();
        assert!(
            var.ends_with("_API_KEY") || var == "GITHUB_TOKEN",
            "{kind:?}: unconventional key var {var}"
        );
    }
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
    // Local ollama's curated list is the :cloud-tag set (see provider_catalog);
    // the two kinds stay DISTINCT lists — cloud never carries :cloud tags.
    assert!(!ProviderKind::Ollama.default_models().is_empty());
    assert!(
        ProviderKind::OllamaCloud
            .default_models()
            .iter()
            .all(|m| !m.ends_with(":cloud") && !m.ends_with("-cloud")),
        "the :cloud suffix is LOCAL-only, never in the hosted catalog"
    );
}

#[test]
fn only_self_hosted_provider_kinds_are_key_optional() {
    let local = [
        ProviderKind::Ollama,
        ProviderKind::LmStudio,
        ProviderKind::LlamaCpp,
        ProviderKind::Vllm,
        ProviderKind::LiteLlm,
    ];
    for kind in ProviderKind::ALL {
        assert_eq!(
            kind.needs_key(),
            !local.contains(&kind),
            "{kind:?} key requirement drifted"
        );
    }
    assert!(ProviderKind::OllamaCloud.needs_key());
}
