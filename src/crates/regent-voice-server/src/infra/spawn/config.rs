//! Regent home, credential backfill, and live call-model selection.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub(crate) fn regent_home() -> PathBuf {
    if let Ok(home) = std::env::var("REGENT_HOME") {
        return PathBuf::from(home);
    }
    let user = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    PathBuf::from(user).join(".regent")
}

pub(super) fn brain_env_from(home: &Path) -> HashMap<String, String> {
    let mut extra = HashMap::new();
    extra.insert("REGENT_HOME".into(), home.to_string_lossy().into_owned());
    if let Ok(dotenv) = std::fs::read_to_string(home.join(".env")) {
        for line in dotenv.lines() {
            let line = line.trim();
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim().trim_matches('"'));
            if !line.starts_with('#') && !key.is_empty() && !value.is_empty() {
                extra.insert(key.to_owned(), value.to_owned());
            }
        }
    }
    extra.retain(|key, _| std::env::var(key).is_err());
    extra
}

/// Voice-only override first; otherwise follow main chat's persisted model.
pub(crate) fn call_model_from(home: &Path) -> Option<String> {
    fn text(value: Option<&serde_yaml::Value>) -> Option<&str> {
        value
            .and_then(serde_yaml::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }
    let raw = std::fs::read_to_string(home.join("config.yaml")).ok()?;
    let doc = serde_yaml::from_str::<serde_yaml::Value>(&raw).ok()?;
    if let Some(fast) = text(
        doc.get("speech")
            .and_then(|speech| speech.get("call"))
            .and_then(|call| call.get("fast_model")),
    ) {
        return Some(fast.to_owned());
    }
    if let Some(primary) = doc
        .get("agents_defaults")
        .and_then(|agents| agents.get("primary"))
        && let (Some(provider), Some(model)) =
            (text(primary.get("provider")), text(primary.get("model")))
    {
        return Some(format!("{provider}/{model}"));
    }
    text(doc.get("model").and_then(|model| model.get("default"))).map(ToOwned::to_owned)
}

pub(super) fn brain_env() -> HashMap<String, String> {
    brain_env_from(&regent_home())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_specific_key_is_preserved_without_generic_alias() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join(".env"), "NVIDIA_API_KEY=nvidia-secret\n").unwrap();
        let env = brain_env_from(home.path());
        assert_eq!(
            env.get("NVIDIA_API_KEY").map(String::as_str),
            Some("nvidia-secret")
        );
        assert!(!env.contains_key("REGENT_API_KEY"));
    }

    #[test]
    fn configured_call_model_overrides_only_the_voice_child() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("config.yaml"),
            "model:\n  default: legacy-model\nagents_defaults:\n  primary: nvidia/main\nspeech:\n  call:\n    fast_model: nvidia/fast\n",
        )
        .unwrap();
        assert_eq!(call_model_from(home.path()).as_deref(), Some("nvidia/fast"));
        let env = brain_env_from(home.path());
        assert!(!env.contains_key("REGENT_MODEL"));
        assert!(!env.contains_key("REGENT_BASE_URL"));
    }

    #[test]
    fn blank_call_model_follows_primary() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("config.yaml"),
            "agents_defaults:\n  primary:\n    provider: nvidia\n    model: nvidia/main\nspeech:\n  call:\n    fast_model: ''\n",
        )
        .unwrap();
        assert_eq!(
            call_model_from(home.path()).as_deref(),
            Some("nvidia/nvidia/main")
        );
    }

    #[test]
    fn call_model_tracks_changes_without_restart() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("config.yaml");
        std::fs::write(
            &path,
            "agents_defaults:\n  primary:\n    provider: nvidia\n    model: nvidia/first\n",
        )
        .unwrap();
        assert_eq!(
            call_model_from(home.path()).as_deref(),
            Some("nvidia/nvidia/first")
        );
        std::fs::write(
            &path,
            "agents_defaults:\n  primary:\n    provider: openrouter\n    model: openai/gpt-next\n",
        )
        .unwrap();
        assert_eq!(
            call_model_from(home.path()).as_deref(),
            Some("openrouter/openai/gpt-next")
        );
    }
}
