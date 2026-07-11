//! `config.set` — the ONLY validated path for the agent to change config.yaml.
//! It sets a dotted scalar path, then proves the WHOLE file still deserializes
//! into `DeaconConfig` (the exact type startup parses, with `deny_unknown_fields`
//! + the provider enum) BEFORE touching disk. So an agent-driven change can
//! never brick the next launch with an invalid enum, a typo'd key, or a wrong
//! type — the write is rejected with that same error instead. Freehand YAML
//! edits (file_edit/terminal) have no such guard, which is why the agent must
//! use this method for config changes.

use super::Dispatcher;
use crate::domain::config::DeaconConfig;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use serde_json::json;
use std::io::ErrorKind;
use std::path::Path;

impl Dispatcher {
    pub(super) fn config_set(&self, req: RpcRequest) {
        let Some(path) = req.params.get("path").and_then(|v| v.as_str()) else {
            self.send(err_response(
                req.id,
                -32602,
                "missing path (dotted, e.g. 'model.provider' or 'context.max_tokens')",
            ));
            return;
        };
        let Some(value) = req.params.get("value") else {
            self.send(err_response(req.id, -32602, "missing value"));
            return;
        };
        let Ok(home) = std::env::var("REGENT_HOME") else {
            self.send(err_response(req.id, -32000, "REGENT_HOME is not set"));
            return;
        };
        match set_config_path(Path::new(&home), path, value) {
            Ok((changed, config)) => {
                // A CUSTOM model applied as primary/fallback (free-text entry —
                // no catalog offers it) joins its provider's `models:` list, so
                // every picker (desktop dropdown, `regent model list`) can offer
                // it from now on instead of it living only in agents_defaults.
                let config = if path.starts_with("agents_defaults") {
                    adopt_custom_models(Path::new(&home), config)
                } else {
                    config
                };
                // Refresh the in-process snapshot + live routing, so the change
                // reaches the NEXT session/turn without a restart.
                self.apply_config(config);
                self.send(ok_response(
                    req.id,
                    json!({
                        "changed": changed,
                        "note": "saved to config.yaml and applied — takes effect from your next message (open sessions re-route too)",
                    }),
                ));
            }
            // Validation failures are the user's/agent's to fix → -32602 with
            // the verbatim serde message (it names the bad enum + valid options).
            Err(e) => self.send(err_response(req.id, -32602, e)),
        }
    }
}

/// Set `dotted.path = value` in config.yaml, VALIDATE the whole file against
/// `DeaconConfig`, and only then write. Returns `("path=value", parsed config)`
/// on success — the caller feeds the parsed config to the live-reload hook —
/// or a human error (invalid enum, unknown key, wrong type) with disk untouched.
/// `pub(super)` so `env.set`'s provider auto-add goes through this same gate
/// instead of growing a second config-write path.
pub(super) fn set_config_path(
    home: &Path,
    path: &str,
    value: &serde_json::Value,
) -> Result<(String, DeaconConfig), String> {
    let file = home.join("config.yaml");
    let raw = match std::fs::read_to_string(&file) {
        Ok(raw) => raw,
        // No config yet → start from the serialized defaults, same as the loader.
        Err(e) if e.kind() == ErrorKind::NotFound => {
            serde_yaml::to_string(&DeaconConfig::default()).map_err(|e| e.to_string())?
        }
        Err(e) => return Err(format!("cannot read config.yaml: {e}")),
    };
    let mut doc: serde_yaml::Value =
        serde_yaml::from_str(&raw).map_err(|e| format!("config.yaml is not valid YAML: {e}"))?;
    let yaml_value = serde_yaml::to_value(value).map_err(|e| e.to_string())?;
    set_path(&mut doc, path, yaml_value)?;
    let out = serde_yaml::to_string(&doc).map_err(|e| e.to_string())?;
    // THE GATE: prove the edited file still parses as the real config type.
    let parsed = serde_yaml::from_str::<DeaconConfig>(&out)
        .map_err(|e| format!("rejected — this would break config.yaml: {e}"))?;
    // Semantic bounds serde can't express (e.g. key_slot within MAX_KEY_SLOTS).
    parsed
        .agents_defaults
        .validate()
        .map_err(|e| format!("rejected — {e}"))?;
    std::fs::write(&file, out).map_err(|e| format!("cannot write config.yaml: {e}"))?;
    Ok((format!("{path}={value}"), parsed))
}

/// Persists any primary/fallback model no catalog offers (neither its
/// provider's configured `models:` nor the kind's curated defaults) into
/// `providers.<name>.models`, through the same validated gate. Fail-open: an
/// adoption failure keeps the already-successful agents_defaults write and the
/// config it produced. Refs naming an unknown provider are skipped — the
/// registry reports those at resolve time.
fn adopt_custom_models(home: &Path, config: DeaconConfig) -> DeaconConfig {
    let refs: Vec<_> = config
        .agents_defaults
        .primary
        .iter()
        .chain(&config.agents_defaults.fallbacks)
        .cloned()
        .collect();
    let mut current = config;
    for r in refs {
        let Some(spec) = current.providers.get(&r.provider) else {
            continue;
        };
        if spec.offers(&r.model) {
            continue;
        }
        let mut models = spec.models.clone();
        models.push(r.model.clone());
        let path = format!("providers.{}.models", r.provider);
        match set_config_path(home, &path, &json!(models)) {
            Ok((_, parsed)) => current = parsed,
            Err(error) => tracing::warn!(%error, model = %r.model, "custom-model adoption failed"),
        }
    }
    current
}

/// Set a dotted path in a YAML mapping, creating intermediate maps as needed.
fn set_path(
    doc: &mut serde_yaml::Value,
    path: &str,
    value: serde_yaml::Value,
) -> Result<(), String> {
    let segments: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return Err("empty path".to_owned());
    }
    let mut cur = doc;
    for seg in &segments[..segments.len() - 1] {
        let map = cur
            .as_mapping_mut()
            .ok_or_else(|| format!("'{seg}' is not a config section"))?;
        let key = serde_yaml::Value::from(*seg);
        if !map.get(&key).is_some_and(serde_yaml::Value::is_mapping) {
            map.insert(key.clone(), serde_yaml::Value::Mapping(Default::default()));
        }
        cur = map.get_mut(&key).unwrap();
    }
    let last = *segments.last().unwrap();
    cur.as_mapping_mut()
        .ok_or_else(|| format!("parent of '{last}' is not a config section"))?
        .insert(serde_yaml::Value::from(last), value);
    Ok(())
}

#[cfg(test)]
mod tests {
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
        let err =
            set_config_path(dir.path(), "model.provider", &json!("ollama-cloud")).unwrap_err();
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
}
