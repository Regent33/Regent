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
    use super::set_config_path;
    use serde_json::json;

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
