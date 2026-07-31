//! Surgical config.yaml edits for the `speech.*` section (used by `voice.set`).
//!
//! This used to parse, edit and `fs::write` the file itself. Nothing validated
//! the result, nothing held the lock, and the write was not atomic — so a
//! Desktop voice change could silently discard a `regent config set` made a
//! moment earlier, and a crash mid-write left a truncated config. It is now the
//! same locked, validated, atomic transaction every other config change uses;
//! all that is left here is naming the leaf keys.

use super::config_ops::set_config_paths;
use serde_json::Value;

/// Set one field (`model` or `provider`) under `speech.asr` / `speech.tts`,
/// leaving every other key as parsed. Returns "what changed" labels.
pub(super) fn set_config_speech_field(
    home: &std::path::Path,
    field: &str,
    asr: Option<&str>,
    tts: Option<&str>,
) -> Result<Vec<String>, String> {
    let mut edits = Vec::new();
    let mut changed = Vec::new();
    for (kind, value) in [("asr", asr), ("tts", tts)] {
        let Some(value) = value else { continue };
        edits.push((
            format!("speech.{kind}.{field}"),
            Value::String(value.to_owned()),
        ));
        changed.push(format!("speech.{kind}.{field}={value} (config.yaml)"));
    }
    if edits.is_empty() {
        return Ok(changed);
    }
    set_config_paths(home, &edits)?;
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::set_config_speech_field;

    #[test]
    fn set_config_models_edits_only_the_model_keys() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("config.yaml"),
            "_config_version: 1\nmodel:\n  default: minimax-m3\nspeech:\n  enabled: true\n  asr:\n    provider: local\n    model: old-asr\n",
        )
        .unwrap();
        let changed =
            set_config_speech_field(dir.path(), "model", Some("new-asr"), Some("new-tts")).unwrap();
        assert_eq!(changed.len(), 2);
        let doc: serde_yaml::Value =
            serde_yaml::from_str(&std::fs::read_to_string(dir.path().join("config.yaml")).unwrap())
                .unwrap();
        assert_eq!(doc["speech"]["asr"]["model"], "new-asr");
        assert_eq!(doc["speech"]["asr"]["provider"], "local", "sibling kept");
        assert_eq!(doc["speech"]["tts"]["model"], "new-tts", "section created");
        assert_eq!(doc["speech"]["enabled"], true);
        assert_eq!(doc["model"]["default"], "minimax-m3", "other sections kept");
    }

    #[test]
    fn set_config_speech_field_edits_only_the_provider_keys() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("config.yaml"),
            "_config_version: 1\nmodel:\n  default: minimax-m3\nspeech:\n  enabled: true\n  asr:\n    provider: local\n    model: old-asr\n",
        )
        .unwrap();
        let changed =
            set_config_speech_field(dir.path(), "provider", Some("openai"), Some("elevenlabs"))
                .unwrap();
        assert_eq!(changed.len(), 2);
        let doc: serde_yaml::Value =
            serde_yaml::from_str(&std::fs::read_to_string(dir.path().join("config.yaml")).unwrap())
                .unwrap();
        assert_eq!(doc["speech"]["asr"]["provider"], "openai");
        assert_eq!(
            doc["speech"]["asr"]["model"], "old-asr",
            "sibling model kept"
        );
        assert_eq!(
            doc["speech"]["tts"]["provider"], "elevenlabs",
            "section created"
        );
        assert_eq!(doc["speech"]["enabled"], true);
        assert_eq!(doc["model"]["default"], "minimax-m3", "other sections kept");
    }

    /// The reason for routing this through the shared gate: a change the schema
    /// cannot accept must bounce, leaving the file exactly as it was.
    #[test]
    fn a_field_the_schema_rejects_leaves_the_file_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "_config_version: 2\nspeech:\n  enabled: true\n").unwrap();
        let before = std::fs::read_to_string(&path).unwrap();
        // `weights` is a list of weight files, not a string.
        let err = set_config_speech_field(dir.path(), "weights", Some("nope"), None).unwrap_err();
        assert!(err.contains("rejected"), "{err}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    }
}
