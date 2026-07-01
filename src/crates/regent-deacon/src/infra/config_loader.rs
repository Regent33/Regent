//! config.yaml loader: serde-deserialise with additive defaults.
//! Secrets (API keys) must live in env vars only — this loader never reads them.

use crate::domain::config::{CURRENT_CONFIG_VERSION, DeaconConfig};
use crate::domain::errors::DeaconError;
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
/// Env overrides (HTTP agent listener) are applied last, so a launcher can enable
/// `/v1/chat` without editing the user's config.yaml.
pub fn load_config(regent_home: &Path) -> Result<DeaconConfig, DeaconError> {
    let path = regent_home.join("config.yaml");
    let mut cfg = if !path.exists() {
        let cfg = DeaconConfig::default();
        save_config(&path, &cfg)?;
        cfg
    } else {
        let raw = std::fs::read_to_string(&path).map_err(DeaconError::Io)?;
        let cfg: DeaconConfig = serde_yaml::from_str(&raw).map_err(DeaconError::Yaml)?;
        if cfg.config_version < CURRENT_CONFIG_VERSION {
            tracing::warn!(
                stored = cfg.config_version,
                current = CURRENT_CONFIG_VERSION,
                "config.yaml version is older than current schema; \
                 missing keys filled with defaults"
            );
        }
        cfg
    };
    apply_http_env_overrides(&mut cfg);
    Ok(cfg)
}

/// Lets a launcher enable the HTTP agent (`/v1/chat`) via env without touching
/// config.yaml: `REGENT_HTTP_ENABLED`, `REGENT_HTTP_BIND`, `REGENT_HTTP_TOKEN`.
/// The token is a per-process value (not an API key), so env is appropriate.
fn apply_http_env_overrides(cfg: &mut DeaconConfig) {
    if let Ok(v) = std::env::var("REGENT_HTTP_ENABLED") {
        cfg.http.enabled = matches!(v.trim(), "1" | "true" | "TRUE" | "yes");
    }
    if let Ok(bind) = std::env::var("REGENT_HTTP_BIND")
        && !bind.trim().is_empty()
    {
        cfg.http.bind = bind;
    }
    if let Ok(token) = std::env::var("REGENT_HTTP_TOKEN")
        && !token.trim().is_empty()
    {
        cfg.http.token = token;
    }
}

fn save_config(path: &Path, cfg: &DeaconConfig) -> Result<(), DeaconError> {
    let yaml = serde_yaml::to_string(cfg).map_err(DeaconError::Yaml)?;
    std::fs::write(path, yaml).map_err(DeaconError::Io)
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
        let mut original = DeaconConfig::default();
        original.model.default = "claude-opus-4-8".to_owned();
        original.cron.tick_interval_secs = 60;
        let yaml = serde_yaml::to_string(&original).unwrap();
        std::fs::write(dir.path().join("config.yaml"), yaml).unwrap();
        let loaded = load_config(dir.path()).unwrap();
        assert_eq!(loaded.model.default, "claude-opus-4-8");
        assert_eq!(loaded.cron.tick_interval_secs, 60);
    }
}
