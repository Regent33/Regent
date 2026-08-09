//! Unit tests for `model_ops` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::{explicit_provider, first_configured_model, split_provider_model};
use crate::domain::config::{DeaconConfig, ProviderSpec};

#[test]
fn splits_on_configured_provider_names_only() {
    let cfg: DeaconConfig = serde_yaml::from_str(
        "_config_version: 1\nproviders:\n  nvidia:\n    kind: nvidia\n    api_key_env: K\n",
    )
    .unwrap();
    // A model id with its own slashes splits at the PROVIDER boundary.
    assert_eq!(
        split_provider_model(&cfg, "nvidia/z-ai/glm-5.2"),
        Some(("nvidia".into(), "z-ai/glm-5.2".into()))
    );
    // Bare catalog ids and unknown prefixes don't persist.
    assert_eq!(split_provider_model(&cfg, "claude-sonnet-4-6"), None);
    assert_eq!(
        split_provider_model(&cfg, "openrouter/minimax/minimax-m3"),
        None
    );
    assert_eq!(split_provider_model(&cfg, "nvidia/"), None);
}

#[test]
fn connectivity_test_skips_blank_poisoned_model_ids() {
    let spec = ProviderSpec {
        models: vec!["".to_owned(), "  ".to_owned(), "llama3.2".to_owned()],
        ..ProviderSpec::default()
    };
    assert_eq!(first_configured_model(&spec), Some("llama3.2"));
}

#[test]
fn explicit_route_fails_closed_when_its_key_is_missing() {
    let cfg: DeaconConfig = serde_yaml::from_str(
        "_config_version: 1\nproviders:\n  private:\n    kind: groq\n    api_key_env: REGENT_TEST_STRICT_ROUTE_KEY_THAT_IS_NOT_SET\n    models: [guard-model]\n",
    )
    .unwrap();
    let error = match explicit_provider(&cfg, "private/guard-model") {
        Ok(_) => panic!("an explicit route must not fall back to the default provider"),
        Err(error) => error,
    };
    assert!(error.contains("private"), "provider is named: {error}");
    assert!(error.contains("REGENT_TEST_STRICT_ROUTE_KEY_THAT_IS_NOT_SET"));
}
