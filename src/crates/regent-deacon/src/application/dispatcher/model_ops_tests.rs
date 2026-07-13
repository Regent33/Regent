//! Unit tests for `model_ops` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::split_provider_model;
use crate::domain::config::DeaconConfig;

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
