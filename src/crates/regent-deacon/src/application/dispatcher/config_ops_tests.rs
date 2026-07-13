//! Unit tests for `config_ops` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::{adopt_custom_models, set_config_path};
use serde_json::json;

// The GLM-5.2 bug: a custom model applied as primary lived only in
// agents_defaults, so no picker (desktop dropdown, `regent model list`)
// ever offered it. Adoption persists it into providers.<name>.models —
// while a curated model (already pickable) is NOT persisted, keeping the
// "defaults are never written back" contract.
#[test]
fn custom_primary_model_joins_the_provider_catalog_and_curated_does_not() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.yaml"),
        "_config_version: 1\nproviders:\n  zhipu:\n    kind: zhipu\n    api_key_env: ZHIPU_API_KEY\n",
    )
    .unwrap();

    // A custom id no catalog offers → adopted into providers.zhipu.models.
    let (_, cfg) = set_config_path(
        dir.path(),
        "agents_defaults.primary",
        &json!({"provider": "zhipu", "model": "glm-5.2-custom"}),
    )
    .unwrap();
    let cfg = adopt_custom_models(dir.path(), cfg);
    assert_eq!(cfg.providers["zhipu"].models, vec!["glm-5.2-custom"]);
    let after = std::fs::read_to_string(dir.path().join("config.yaml")).unwrap();
    assert!(after.contains("glm-5.2-custom"), "persisted: {after}");

    // A CURATED id (zhipu's defaults list glm-5.2) is already pickable —
    // adoption must not write it back.
    let (_, cfg) = set_config_path(
        dir.path(),
        "agents_defaults.primary",
        &json!({"provider": "zhipu", "model": "glm-5.2"}),
    )
    .unwrap();
    let cfg = adopt_custom_models(dir.path(), cfg);
    assert_eq!(
        cfg.providers["zhipu"].models,
        vec!["glm-5.2-custom"],
        "curated ids are never persisted"
    );

    // An unknown provider is skipped, not an error.
    let (_, cfg) = set_config_path(
        dir.path(),
        "agents_defaults.primary",
        &json!({"provider": "nope", "model": "x"}),
    )
    .unwrap();
    let _ = adopt_custom_models(dir.path(), cfg);
}

#[test]
fn valid_provider_writes_and_bad_provider_is_rejected_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.yaml"),
        "_config_version: 1\nmodel:\n  default: claude-sonnet-4-6\n  provider: openrouter\n",
    )
    .unwrap();

    // A known provider round-trips and persists — and hands back the
    // parsed config for the live-reload hook.
    let (ok, parsed) = set_config_path(dir.path(), "model.provider", &json!("ollama")).unwrap();
    assert_eq!(ok, "model.provider=\"ollama\"");
    assert_eq!(
        parsed.model.provider,
        crate::domain::config::ProviderKind::Ollama
    );
    let after = std::fs::read_to_string(dir.path().join("config.yaml")).unwrap();
    assert!(after.contains("provider: ollama"));

    // The exact failure the user hit: an invalid enum must be refused, and
    // the file must be left byte-identical (no partial/bricking write).
    let before = after.clone();
    let err = set_config_path(dir.path(), "model.provider", &json!("ollama-cloud")).unwrap_err();
    assert!(
        err.contains("unknown variant") && err.contains("ollama-cloud"),
        "{err}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("config.yaml")).unwrap(),
        before
    );
}

#[test]
fn creates_intermediate_sections_and_validates_types() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.yaml"), "_config_version: 1\n").unwrap();
    // A numeric field set through a section that must be created.
    set_config_path(dir.path(), "context.max_tokens", &json!(120000)).unwrap();
    let after = std::fs::read_to_string(dir.path().join("config.yaml")).unwrap();
    assert!(after.contains("max_tokens: 120000"));
    // A string where a number belongs is rejected by the type gate.
    assert!(set_config_path(dir.path(), "context.max_tokens", &json!("lots")).is_err());
}
