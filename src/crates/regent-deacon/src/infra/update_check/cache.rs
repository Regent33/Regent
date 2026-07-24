//! Bounded on-disk update cache at `$REGENT_HOME/update/cache.json`.
//!
//! It stores only compact facts — the ETag to replay, timestamps, the latest
//! advertised version, and a short diagnostic — never the raw manifest, so a
//! hostile or oversized response can never bloat the cache. A missing or corrupt
//! cache reads as `None` (the checker then behaves as a first-ever run).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Longest diagnostic we persist (characters); anything longer is truncated.
pub const MAX_DIAGNOSTIC_CHARS: usize = 300;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheFile {
    /// ETag from the last successful fetch, replayed as `If-None-Match`.
    #[serde(default)]
    pub etag: Option<String>,
    /// Unix seconds of the last completed check attempt (success or failure).
    /// `0` means "never checked".
    #[serde(default)]
    pub checked_at: i64,
    /// Latest stable version the manifest advertised, if any.
    #[serde(default)]
    pub latest: Option<String>,
    /// Short, bounded reason the last check failed (kept for `doctor`).
    #[serde(default)]
    pub diagnostic: Option<String>,
}

impl CacheFile {
    #[must_use]
    pub fn path(home: &Path) -> PathBuf {
        home.join("update").join("cache.json")
    }

    /// Read the cache; any error (missing dir, corrupt JSON) yields `None`.
    #[must_use]
    pub fn load(home: &Path) -> Option<Self> {
        let raw = std::fs::read(Self::path(home)).ok()?;
        serde_json::from_slice(&raw).ok()
    }

    /// Persist via a same-directory temporary file. Unix replaces atomically;
    /// Windows requires removing the old cache first, but a crash can only lose
    /// this optional cache — never user state or the running installation.
    pub fn store(&self, home: &Path) -> std::io::Result<()> {
        let mut cache = self.clone();
        cache.diagnostic = cache.diagnostic.map(|diagnostic| bound(&diagnostic));
        let path = Self::path(home);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let body = serde_json::to_vec_pretty(&cache).unwrap_or_default();
        let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        std::fs::write(&tmp, &body)?;
        match std::fs::rename(&tmp, &path) {
            Ok(()) => Ok(()),
            Err(_) if path.exists() => {
                std::fs::remove_file(&path)?;
                std::fs::rename(&tmp, &path)
            }
            Err(error) => {
                let _ = std::fs::remove_file(&tmp);
                Err(error)
            }
        }
    }
}

/// Truncate to `MAX_DIAGNOSTIC_CHARS` characters (never splits a UTF-8 char).
#[must_use]
pub fn bound(s: &str) -> String {
    s.chars().take(MAX_DIAGNOSTIC_CHARS).collect()
}
