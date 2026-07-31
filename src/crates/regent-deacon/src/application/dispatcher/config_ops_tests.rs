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
    //
    // This used to say "ollama-cloud", which is now a real kind — so it would
    // pass for the wrong reason. The guard is the *behaviour*, not the value:
    // any plausible-looking name that isn't a variant must still bounce.
    let before = after.clone();
    let err = set_config_path(dir.path(), "model.provider", &json!("ollama-hosted")).unwrap_err();
    assert!(
        err.contains("unknown variant") && err.contains("ollama-hosted"),
        "{err}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("config.yaml")).unwrap(),
        before
    );
}

/// A batch is one transaction. `regent setup` sets four related keys at once;
/// if a refusal on the last one could leave the first three applied, an
/// install would be half-configured with no command that says so.
#[test]
fn a_batch_applies_every_key_or_none_of_them() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.yaml"),
        "_config_version: 2\nmodel:\n  default: keep-me\n  provider: anthropic\n",
    )
    .unwrap();
    let before = std::fs::read_to_string(dir.path().join("config.yaml")).unwrap();

    // Good key FIRST, bad key second — the order that catches a loop applying
    // each key in its own transaction.
    let err = super::set_config_paths(
        dir.path(),
        &[
            ("model.default".to_owned(), json!("clobbered")),
            ("model.provider".to_owned(), json!("notaprovider")),
        ],
    )
    .unwrap_err();
    assert!(err.contains("unknown variant"), "{err}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("config.yaml")).unwrap(),
        before,
        "a refused batch must leave the file byte-identical"
    );

    // …and an all-good batch applies every key in one write.
    let (changed, parsed) = super::set_config_paths(
        dir.path(),
        &[
            ("model.default".to_owned(), json!("llama3.2")),
            ("model.provider".to_owned(), json!("ollama")),
            ("model.base_url".to_owned(), json!(null)),
        ],
    )
    .unwrap();
    assert!(changed.contains("model.default") && changed.contains("model.provider"));
    assert_eq!(parsed.model.default, "llama3.2");
    assert_eq!(parsed.model.base_url, None); // null CLEARS, it does not stringify
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

/// The gate must refuse a scalar where the schema wants a list. `regent config
/// set tools.pinned '["read_file"]'` reported SUCCESS and left the file holding
/// `pinned: '["read_file"]'` — after which the deacon would not start at all:
///
///   fatal: yaml: tools.pinned: invalid type: string "[\"read_file\"]",
///          expected a sequence
///
/// Reproduced against a real config. The CLI side is fixed (coerce now parses
/// JSON arrays), but this is the layer whose whole job is to prove the edited
/// file still loads, so it has to hold the line on its own.
#[test]
fn a_scalar_where_the_schema_wants_a_list_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.yaml"),
        serde_yaml::to_string(&crate::domain::config::DeaconConfig::default()).unwrap(),
    )
    .unwrap();
    let before = std::fs::read_to_string(dir.path().join("config.yaml")).unwrap();

    let result = set_config_path(dir.path(), "tools.pinned", &json!("[\"read_file\"]"));

    assert!(
        result.is_err(),
        "the gate accepted a string for tools.pinned (Vec<String>) — the deacon \
         cannot load the file it just wrote"
    );
    // A refused write must not touch the file either.
    assert_eq!(
        std::fs::read_to_string(dir.path().join("config.yaml")).unwrap(),
        before,
        "a rejected edit still rewrote config.yaml"
    );
}
