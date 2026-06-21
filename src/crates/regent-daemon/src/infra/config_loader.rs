//! config.yaml loader: serde-deserialise with additive defaults.
//! Secrets (API keys) must live in env vars only — this loader never reads them.

use crate::domain::config::{CURRENT_CONFIG_VERSION, DaemonConfig};
use crate::domain::errors::DaemonError;
use std::path::{Path, PathBuf};

/// Resolves `~` to the user home directory on the leading segment.
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_default();
        PathBuf::from(home).join(rest)
    } else if path == "~" {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_default();
        PathBuf::from(home)
    } else {
        PathBuf::from(path)
    }
}

/// Loads `$REGENT_HOME/config.yaml`. Creates it with defaults when absent.
/// Warn-logs when the stored version is older than the current schema version.
pub fn load_config(regent_home: &Path) -> Result<DaemonConfig, DaemonError> {
    let path = regent_home.join("config.yaml");
    if !path.exists() {
        let cfg = DaemonConfig::default();
        save_config(&path, &cfg)?;
        return Ok(cfg);
    }
    let raw = std::fs::read_to_string(&path).map_err(DaemonError::Io)?;
    let cfg: DaemonConfig = serde_yaml::from_str(&raw).map_err(DaemonError::Yaml)?;
    if cfg.config_version < CURRENT_CONFIG_VERSION {
        tracing::warn!(
            stored = cfg.config_version,
            current = CURRENT_CONFIG_VERSION,
            "config.yaml version is older than current schema; \
             missing keys filled with defaults"
        );
    }
    Ok(cfg)
}

fn save_config(path: &Path, cfg: &DaemonConfig) -> Result<(), DaemonError> {
    let yaml = serde_yaml::to_string(cfg).map_err(DaemonError::Yaml)?;
    std::fs::write(path, yaml).map_err(DaemonError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn missing_config_creates_default() {
        let dir = TempDir::new().unwrap();
        let cfg = load_config(dir.path()).unwrap();
        assert_eq!(cfg.config_version, CURRENT_CONFIG_VERSION);
        assert!(dir.path().join("config.yaml").exists());
    }

    #[test]
    fn partial_yaml_fills_defaults() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("config.yaml"), "_config_version: 1\n").unwrap();
        let cfg = load_config(dir.path()).unwrap();
        assert_eq!(cfg.model.default, "claude-sonnet-4-6");
        assert_eq!(cfg.cron.tick_interval_secs, 30);
    }

    #[test]
    fn unknown_keys_are_a_hard_error_never_a_silent_default() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("config.yaml"),
            "_config_version: 1\nmodel:\n  defalut: \"typo\"\n",
        )
        .unwrap();
        assert!(load_config(dir.path()).is_err());
    }

    #[test]
    fn round_trip_preserves_values() {
        let dir = TempDir::new().unwrap();
        let mut original = DaemonConfig::default();
        original.model.default = "claude-opus-4-8".to_owned();
        original.cron.tick_interval_secs = 60;
        let yaml = serde_yaml::to_string(&original).unwrap();
        std::fs::write(dir.path().join("config.yaml"), yaml).unwrap();
        let loaded = load_config(dir.path()).unwrap();
        assert_eq!(loaded.model.default, "claude-opus-4-8");
        assert_eq!(loaded.cron.tick_interval_secs, 60);
    }
}
