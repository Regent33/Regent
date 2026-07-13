//! Model manager — resolve a model to a local cache dir, download + verify it,
//! and skip work when it's already present. The explicit, gated form of
//! `regent-embed`'s fastembed auto-download: nothing here runs until
//! `regent voice setup` asks for it.
//!
//! The network is **injected** as a `fetch` closure rather than depended on, so
//! the manager is pure (filesystem + hashing) and fully unit-testable; the
//! deacon wires the real HTTPS fetcher (reqwest, with progress) at the call
//! site.

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Which speech kind a model serves — picks its cache sub-directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    Asr,
    Tts,
}

impl ModelKind {
    #[must_use]
    pub fn dir(self) -> &'static str {
        match self {
            Self::Asr => "asr",
            Self::Tts => "tts",
        }
    }
}

/// One file belonging to a model: where to get it and its expected digest.
/// An empty `sha256` means "no published digest" — the file is trusted as
/// downloaded (some sources don't publish hashes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFile {
    pub name: String,
    pub url: String,
    pub sha256: String,
}

/// A model to ensure present: its kind, id, and constituent files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSpec {
    pub kind: ModelKind,
    pub id: String,
    pub files: Vec<ModelFile>,
}

#[derive(Debug, Error)]
pub enum ManagerError {
    #[error("download failed: {0}")]
    Fetch(String),
    #[error("checksum mismatch for {file}: expected {expected}, got {actual}")]
    Checksum {
        file: String,
        expected: String,
        actual: String,
    },
    #[error("io error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// A model `id` or file `name` was not a plain relative filename — rejected
    /// before any filesystem access (path-traversal / arbitrary-write defense).
    #[error("unsafe model path component {value:?}: must be a plain relative filename")]
    UnsafeName { value: String },
}

/// Owns the model cache root (`$REGENT_HOME/models`).
pub struct ModelManager {
    root: PathBuf,
}

impl ModelManager {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `<root>/<kind>/<id>` — where a model's files live.
    #[must_use]
    pub fn model_dir(&self, kind: ModelKind, id: &str) -> PathBuf {
        self.root.join(kind.dir()).join(id)
    }

    /// True when every file of `spec` exists and matches its digest. An unsafe
    /// `id`/`name` is treated as not-present (it can never be written), so the
    /// caller proceeds to `ensure`, which rejects it.
    #[must_use]
    pub fn is_present(&self, spec: &ModelSpec) -> bool {
        if !is_safe_component(&spec.id) || spec.files.iter().any(|f| !is_safe_component(&f.name)) {
            return false;
        }
        let dir = self.model_dir(spec.kind, &spec.id);
        spec.files
            .iter()
            .all(|f| file_matches(&dir.join(&f.name), &f.sha256))
    }

    /// Ensure every file of `spec` is present and verified, downloading the
    /// missing/invalid ones via `fetch`. Idempotent: a fully-present model does
    /// no I/O and never calls `fetch`. A checksum mismatch fails without leaving
    /// the bad bytes in place. Returns the model directory.
    pub fn ensure<F>(&self, spec: &ModelSpec, fetch: F) -> Result<PathBuf, ManagerError>
    where
        F: Fn(&str) -> Result<Vec<u8>, String>,
    {
        // Path-traversal defense: reject before any filesystem access or fetch.
        // `id` and every `name` come from config (agent-writable) and become
        // path components — an `id`/`name` with `..` or a separator could escape
        // the model cache and overwrite arbitrary files.
        if !is_safe_component(&spec.id) {
            return Err(ManagerError::UnsafeName {
                value: spec.id.clone(),
            });
        }
        for f in &spec.files {
            if !is_safe_component(&f.name) {
                return Err(ManagerError::UnsafeName {
                    value: f.name.clone(),
                });
            }
        }
        let dir = self.model_dir(spec.kind, &spec.id);
        if self.is_present(spec) {
            return Ok(dir);
        }
        create_dir_all(&dir)?;
        for f in &spec.files {
            let path = dir.join(&f.name);
            if file_matches(&path, &f.sha256) {
                continue; // resume a partial download — skip good files
            }
            let bytes = fetch(&f.url).map_err(ManagerError::Fetch)?;
            let actual = sha256_hex(&bytes);
            if !f.sha256.is_empty() && actual != f.sha256 {
                return Err(ManagerError::Checksum {
                    file: f.name.clone(),
                    expected: f.sha256.clone(),
                    actual,
                });
            }
            write_atomic(&path, &bytes)?;
        }
        Ok(dir)
    }
}

/// Lowercase hex SHA-256 of `bytes`.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// True when `s` is a safe single path component: a plain relative name, no
/// separators, no `.`/`..`, no drive/ADS colon, no NUL. Guards the model `id`
/// and file `name` (both config-sourced, agent-writable) against escaping the
/// model cache dir.
fn is_safe_component(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && !s.contains('/')
        && !s.contains('\\')
        && !s.contains(':')
        && !s.contains('\0')
}

/// True when `path` exists and (if `expected` is non-empty) its digest matches.
fn file_matches(path: &Path, expected: &str) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    expected.is_empty() || sha256_hex(&bytes) == expected
}

fn create_dir_all(dir: &Path) -> Result<(), ManagerError> {
    fs::create_dir_all(dir).map_err(|source| ManagerError::Io {
        path: dir.display().to_string(),
        source,
    })
}

/// Write `bytes` to `path` atomically (temp file in the same dir, then rename),
/// so an interrupted write never leaves a half-file that would pass an
/// empty-digest "present" check.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ManagerError> {
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    let io = |source: std::io::Error| ManagerError::Io {
        path: path.display().to_string(),
        source,
    };
    fs::write(&tmp, bytes).map_err(io)?;
    fs::rename(&tmp, path).map_err(io)
}

#[cfg(test)]
#[path = "models_tests.rs"]
mod tests;
