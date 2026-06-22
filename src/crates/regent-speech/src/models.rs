//! Model manager — resolve a model to a local cache dir, download + verify it,
//! and skip work when it's already present. The explicit, gated form of
//! `regent-embed`'s fastembed auto-download: nothing here runs until
//! `regent voice setup` asks for it.
//!
//! The network is **injected** as a `fetch` closure rather than depended on, so
//! the manager is pure (filesystem + hashing) and fully unit-testable; the
//! daemon wires the real HTTPS fetcher (reqwest, with progress) at the call
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

    /// True when every file of `spec` exists and matches its digest.
    #[must_use]
    pub fn is_present(&self, spec: &ModelSpec) -> bool {
        let dir = self.model_dir(spec.kind, &spec.id);
        spec.files.iter().all(|f| file_matches(&dir.join(&f.name), &f.sha256))
    }

    /// Ensure every file of `spec` is present and verified, downloading the
    /// missing/invalid ones via `fetch`. Idempotent: a fully-present model does
    /// no I/O and never calls `fetch`. A checksum mismatch fails without leaving
    /// the bad bytes in place. Returns the model directory.
    pub fn ensure<F>(&self, spec: &ModelSpec, fetch: F) -> Result<PathBuf, ManagerError>
    where
        F: Fn(&str) -> Result<Vec<u8>, String>,
    {
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
mod tests {
    use super::*;

    fn spec(files: Vec<ModelFile>) -> ModelSpec {
        ModelSpec {
            kind: ModelKind::Asr,
            id: "qwen3-asr".into(),
            files,
        }
    }

    #[test]
    fn model_dir_layout_is_kind_then_id() {
        let mgr = ModelManager::new("/models");
        assert_eq!(
            mgr.model_dir(ModelKind::Tts, "qwen3-tts"),
            Path::new("/models/tts/qwen3-tts")
        );
    }

    #[test]
    fn sha256_hex_is_stable() {
        // Known SHA-256 of "abc".
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn ensure_downloads_then_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = ModelManager::new(tmp.path());
        let body = b"weights".to_vec();
        let s = spec(vec![ModelFile {
            name: "model.bin".into(),
            url: "https://example/model.bin".into(),
            sha256: sha256_hex(&body),
        }]);
        assert!(!mgr.is_present(&s));

        let calls = std::cell::Cell::new(0);
        let fetch = |_url: &str| {
            calls.set(calls.get() + 1);
            Ok(body.clone())
        };
        let dir = mgr.ensure(&s, fetch).unwrap();
        assert!(dir.join("model.bin").is_file());
        assert!(mgr.is_present(&s));
        assert_eq!(calls.get(), 1);

        // Second call: already present → no fetch.
        mgr.ensure(&s, fetch).unwrap();
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn ensure_rejects_a_checksum_mismatch_without_storing() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = ModelManager::new(tmp.path());
        let s = spec(vec![ModelFile {
            name: "model.bin".into(),
            url: "https://example/model.bin".into(),
            sha256: sha256_hex(b"expected"),
        }]);
        let err = mgr.ensure(&s, |_| Ok(b"corrupted".to_vec())).unwrap_err();
        assert!(matches!(err, ManagerError::Checksum { .. }));
        assert!(!mgr.is_present(&s));
    }

    #[test]
    fn empty_digest_skips_verification() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = ModelManager::new(tmp.path());
        let s = spec(vec![ModelFile {
            name: "m.bin".into(),
            url: "u".into(),
            sha256: String::new(),
        }]);
        mgr.ensure(&s, |_| Ok(b"anything".to_vec())).unwrap();
        assert!(mgr.is_present(&s));
    }
}
