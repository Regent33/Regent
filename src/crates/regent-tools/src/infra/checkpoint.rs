//! File-state checkpoints (P7 ops): snapshot a set of files before a risky
//! edit, then roll back to restore them. A checkpoint copies each file's
//! current bytes (or records that it was absent) under the store root; rollback
//! rewrites the bytes — or deletes a file that didn't exist at snapshot time —
//! so a botched edit (or a whole turn) is recoverable. Rows are never deleted.

use regent_kernel::RegentError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize)]
struct Entry {
    original: PathBuf,
    /// Whether the file existed at snapshot time. False → rollback deletes it.
    existed: bool,
    /// Copied-content filename within the checkpoint dir (when `existed`).
    blob: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    id: String,
    label: String,
    created_at: f64,
    entries: Vec<Entry>,
}

/// One checkpoint's summary (for `list`).
#[derive(Debug, Clone, PartialEq)]
pub struct CheckpointInfo {
    pub id: String,
    pub label: String,
    pub created_at: f64,
    pub file_count: usize,
}

/// Filesystem-backed checkpoint store rooted at a directory (e.g.
/// `$REGENT_HOME/checkpoints`).
pub struct CheckpointStore {
    root: PathBuf,
}

fn tool_err(message: impl Into<String>) -> RegentError {
    RegentError::Tool { tool: "checkpoint".into(), message: message.into() }
}

fn now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

impl CheckpointStore {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Snapshots the current state of `paths`, returning the checkpoint id.
    pub fn snapshot(&self, label: &str, paths: &[PathBuf]) -> Result<String, RegentError> {
        let id = format!("ckpt_{}", uuid::Uuid::new_v4().simple());
        let dir = self.root.join(&id);
        fs::create_dir_all(&dir).map_err(|e| tool_err(e.to_string()))?;

        let mut entries = Vec::with_capacity(paths.len());
        for (i, path) in paths.iter().enumerate() {
            if path.is_file() {
                let blob = format!("{i}.blob");
                fs::copy(path, dir.join(&blob)).map_err(|e| tool_err(e.to_string()))?;
                entries.push(Entry { original: path.clone(), existed: true, blob: Some(blob) });
            } else {
                entries.push(Entry { original: path.clone(), existed: false, blob: None });
            }
        }

        let manifest = Manifest { id: id.clone(), label: label.to_owned(), created_at: now(), entries };
        let bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| tool_err(e.to_string()))?;
        fs::write(dir.join("manifest.json"), bytes).map_err(|e| tool_err(e.to_string()))?;
        Ok(id)
    }

    /// Restores every file recorded in checkpoint `id` to its snapshot state.
    pub fn rollback(&self, id: &str) -> Result<(), RegentError> {
        let manifest = self.read_manifest(id)?;
        for entry in &manifest.entries {
            match (entry.existed, &entry.blob) {
                (true, Some(blob)) => {
                    if let Some(parent) = entry.original.parent() {
                        fs::create_dir_all(parent).map_err(|e| tool_err(e.to_string()))?;
                    }
                    fs::copy(self.root.join(id).join(blob), &entry.original)
                        .map_err(|e| tool_err(e.to_string()))?;
                }
                // Didn't exist at snapshot → remove it if the edit created it.
                _ if entry.original.is_file() => {
                    fs::remove_file(&entry.original).map_err(|e| tool_err(e.to_string()))?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Lists checkpoints newest first.
    pub fn list(&self) -> Result<Vec<CheckpointInfo>, RegentError> {
        let mut out = Vec::new();
        let Ok(read_dir) = fs::read_dir(&self.root) else { return Ok(out) };
        for entry in read_dir.flatten() {
            if let Some(id) = entry.file_name().to_str()
                && let Ok(m) = self.read_manifest(id)
            {
                out.push(CheckpointInfo {
                    id: m.id,
                    label: m.label,
                    created_at: m.created_at,
                    file_count: m.entries.len(),
                });
            }
        }
        out.sort_by(|a, b| b.created_at.total_cmp(&a.created_at));
        Ok(out)
    }

    fn read_manifest(&self, id: &str) -> Result<Manifest, RegentError> {
        let bytes = fs::read(self.root.join(id).join("manifest.json"))
            .map_err(|e| tool_err(format!("unknown checkpoint '{id}': {e}")))?;
        serde_json::from_slice(&bytes).map_err(|e| tool_err(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, CheckpointStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path().join("checkpoints"));
        (dir, store)
    }

    #[test]
    fn rollback_restores_modified_content() {
        let (dir, store) = store();
        let file = dir.path().join("notes.txt");
        fs::write(&file, "original").unwrap();

        let id = store.snapshot("edit notes", std::slice::from_ref(&file)).unwrap();
        fs::write(&file, "clobbered").unwrap();
        store.rollback(&id).unwrap();

        assert_eq!(fs::read_to_string(&file).unwrap(), "original");
    }

    #[test]
    fn rollback_deletes_a_file_that_did_not_exist_at_snapshot() {
        let (dir, store) = store();
        let file = dir.path().join("new.txt");

        let id = store.snapshot("create file", std::slice::from_ref(&file)).unwrap();
        fs::write(&file, "freshly created").unwrap();
        store.rollback(&id).unwrap();

        assert!(!file.exists(), "a file created after the snapshot is removed on rollback");
    }

    #[test]
    fn list_reports_checkpoints_and_unknown_rollback_errors() {
        let (dir, store) = store();
        let file = dir.path().join("a.txt");
        fs::write(&file, "x").unwrap();
        store.snapshot("first", &[file]).unwrap();

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].label, "first");
        assert_eq!(listed[0].file_count, 1);

        assert!(store.rollback("ckpt_does_not_exist").is_err());
    }
}
