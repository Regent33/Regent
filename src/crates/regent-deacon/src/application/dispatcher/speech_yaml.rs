//! Surgical config.yaml edits for the `speech.*` section (used by `voice.set`).

/// Set one field (`model` or `provider`) under `speech.asr` / `speech.tts`,
/// leaving every other key as parsed. Returns "what changed" labels.
pub(super) fn set_config_speech_field(
    home: &std::path::Path,
    field: &str,
    asr: Option<&str>,
    tts: Option<&str>,
) -> Result<Vec<String>, String> {
    let path = home.join("config.yaml");
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut doc: serde_yaml::Value =
        serde_yaml::from_str(&raw).map_err(|e| format!("config.yaml: {e}"))?;
    let mut changed = Vec::new();
    for (kind, value) in [("asr", asr), ("tts", tts)] {
        let Some(value) = value else { continue };
        let speech = ensure_map(&mut doc, "speech")?;
        let section = ensure_map(speech, kind)?;
        section
            .as_mapping_mut()
            .unwrap()
            .insert(field.into(), value.into());
        changed.push(format!("speech.{kind}.{field}={value} (config.yaml)"));
    }
    let out = serde_yaml::to_string(&doc).map_err(|e| e.to_string())?;
    std::fs::write(&path, out).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(changed)
}

/// Get `key` as a mapping inside `doc`, creating/replacing as needed.
fn ensure_map<'a>(
    doc: &'a mut serde_yaml::Value,
    key: &str,
) -> Result<&'a mut serde_yaml::Value, String> {
    let map = doc
        .as_mapping_mut()
        .ok_or_else(|| "config.yaml is not a mapping".to_owned())?;
    let k = serde_yaml::Value::from(key);
    if !map.get(&k).is_some_and(serde_yaml::Value::is_mapping) {
        map.insert(k.clone(), serde_yaml::Value::Mapping(Default::default()));
    }
    Ok(map.get_mut(&k).unwrap())
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
}
