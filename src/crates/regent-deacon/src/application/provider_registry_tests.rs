//! Unit tests for `provider_registry` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use crate::domain::config::ProviderKind;

fn spec(kind: ProviderKind, key_env: &str, models: &[&str]) -> ProviderSpec {
    ProviderSpec {
        kind,
        base_url: None,
        api_key_env: key_env.to_owned(),
        models: models.iter().map(|s| (*s).to_owned()).collect(),
    }
}

fn registry(key_env: &str) -> ProviderRegistry {
    let mut specs = HashMap::new();
    specs.insert(
        "groq".to_owned(),
        spec(ProviderKind::Groq, key_env, &["llama-3.3-70b"]),
    );
    ProviderRegistry::from_config(&specs)
}

#[test]
fn resolves_and_memoizes_a_known_provider() {
    // Unique env var name → no cross-test interference under parallel runs.
    let env = "REGENT_TEST_KEY_RESOLVES";
    unsafe { std::env::set_var(env, "secret") };
    let reg = registry(env);
    let m = ModelRef::new("groq", "llama-3.3-70b");
    let p1 = reg.provider_for(&m).unwrap();
    assert_eq!(p1.model(), "llama-3.3-70b");
    let p2 = reg.provider_for(&m).unwrap();
    assert!(Arc::ptr_eq(&p1, &p2), "second resolve is a cache hit");
    unsafe { std::env::remove_var(env) };
}

#[test]
fn unknown_provider_is_a_typed_error() {
    // matches! on the Result: Arc<dyn ChatProvider> isn't Debug, so no unwrap_err.
    let reg = registry("REGENT_TEST_KEY_UNKNOWN");
    let res = reg.provider_for(&ModelRef::new("nope", "x"));
    assert!(matches!(res, Err(RegistryError::UnknownProvider(p)) if p == "nope"));
}

#[test]
fn missing_key_is_a_typed_error() {
    // This env var is never set anywhere → MissingKey.
    let reg = registry("REGENT_TEST_KEY_DEFINITELY_UNSET");
    let res = reg.provider_for(&ModelRef::new("groq", "llama-3.3-70b"));
    assert!(
        matches!(res, Err(RegistryError::MissingKey { ref env, .. }) if env == "REGENT_TEST_KEY_DEFINITELY_UNSET")
    );
}

#[test]
fn keyless_provider_resolves_with_an_empty_key() {
    // A provider with no `api_key_env` (local Ollama on localhost) is keyless
    // — it must resolve with NO env var set, not error as MissingKey.
    let mut specs = HashMap::new();
    specs.insert(
        "ollama-local".to_owned(),
        spec(ProviderKind::Ollama, "", &[]),
    );
    let reg = ProviderRegistry::from_config(&specs);
    assert!(
        reg.provider_for(&ModelRef::new("ollama-local", "llama3"))
            .is_ok(),
        "keyless provider resolves without any key"
    );
}

#[test]
fn key_slot_pins_the_slotted_var_and_memoizes_separately() {
    let env = "REGENT_TEST_KEY_SLOTTED";
    unsafe {
        std::env::set_var(env, "key-one");
        std::env::set_var(format!("{env}_2"), "key-two");
    }
    let reg = registry(env);
    let base = ModelRef::new("groq", "llama-3.3-70b");
    // Same provider+model on slot 2 is a DIFFERENT chain link (multi-key
    // failover) — distinct cache entries, both resolvable.
    let p1 = reg.provider_for(&base).unwrap();
    let p2 = reg.provider_for(&base.clone().with_key_slot(2)).unwrap();
    assert!(!Arc::ptr_eq(&p1, &p2), "slots resolve independently");
    // An unset slot is the same typed error a missing base key produces,
    // naming the exact slotted var.
    let res = reg.provider_for(&base.with_key_slot(7));
    assert!(
        matches!(res, Err(RegistryError::MissingKey { ref env, .. }) if env == "REGENT_TEST_KEY_SLOTTED_7")
    );
    unsafe {
        std::env::remove_var(env);
        std::env::remove_var(format!("{env}_2"));
    }
}

#[test]
fn chain_for_skips_a_fallback_whose_slot_is_unset() {
    let env = "REGENT_TEST_KEY_SLOTGAP";
    unsafe { std::env::set_var(env, "secret") };
    let reg = registry(env);
    let primary = ModelRef::new("groq", "llama-3.3-70b");
    // Slot 5 never set → that fallback is skipped, chain degrades to primary.
    let chain = reg
        .chain_for(&primary, &[primary.clone().with_key_slot(5)], None)
        .unwrap();
    assert_eq!(chain.model(), "llama-3.3-70b");
    unsafe { std::env::remove_var(env) };
}

#[test]
fn chain_for_builds_a_fallback_when_fallbacks_resolve() {
    let env = "REGENT_TEST_KEY_CHAIN";
    unsafe { std::env::set_var(env, "secret") };
    let mut specs = HashMap::new();
    specs.insert("a".to_owned(), spec(ProviderKind::Groq, env, &["m1"]));
    specs.insert("b".to_owned(), spec(ProviderKind::Groq, env, &["m2"]));
    let reg = ProviderRegistry::from_config(&specs);
    let chain = reg
        .chain_for(&ModelRef::new("a", "m1"), &[ModelRef::new("b", "m2")], None)
        .unwrap();
    assert_eq!(chain.model(), "m1", "primary serves first");
    unsafe { std::env::remove_var(env) };
}

#[test]
fn resolve_model_str_prefers_a_configured_provider_prefix() {
    let reg = registry("REGENT_TEST_KEY_PARSE");
    // "groq/" prefix is a configured provider → split.
    let m = reg.resolve_model_str("groq/llama-3.3-70b", None).unwrap();
    assert_eq!(m, ModelRef::new("groq", "llama-3.3-70b"));
    // No configured prefix, no default → None.
    assert!(
        reg.resolve_model_str("anthropic/claude-opus-4-8", None)
            .is_none()
    );
    // Bare/slashed id with a default → whole spec is the model id under the default provider.
    let dflt = ModelRef::new("openrouter", "x");
    let m = reg
        .resolve_model_str("anthropic/claude-opus-4-8", Some(&dflt))
        .unwrap();
    assert_eq!(m, ModelRef::new("openrouter", "anthropic/claude-opus-4-8"));
}

#[test]
fn resolve_model_str_prefers_the_provider_that_lists_the_model() {
    // "or" LISTS the org-prefixed id; "local" is the primary. The spec must
    // resolve to "or", not get pinned onto the primary (which would 404).
    let mut specs = HashMap::new();
    specs.insert(
        "or".to_owned(),
        spec(ProviderKind::OpenRouter, "K", &["minimax/minimax-m3"]),
    );
    specs.insert(
        "local".to_owned(),
        spec(ProviderKind::Ollama, "K", &["minimax-m3"]),
    );
    let reg = ProviderRegistry::from_config(&specs);
    let primary = ModelRef::new("local", "minimax-m3");
    let m = reg
        .resolve_model_str("minimax/minimax-m3", Some(&primary))
        .unwrap();
    assert_eq!(m, ModelRef::new("or", "minimax/minimax-m3"));
    // A bare id listed by a provider resolves there too.
    let m = reg.resolve_model_str("minimax-m3", Some(&primary)).unwrap();
    assert_eq!(m, ModelRef::new("local", "minimax-m3"));
}
