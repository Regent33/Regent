//! Git-backed working-tree checkpoint for revert-to-last-green. Snapshot
//! captures the pre-execute tree state (preserving any pre-existing uncommitted
//! work); restore rewinds tracked modifications/deletions and removes files the
//! execute phase newly created. Outside a git repo, snapshot returns `None` so
//! the harness degrades to report-only rather than guessing.

use crate::application::Checkpoint;
use async_trait::async_trait;
use regent_kernel::RegentError;
use std::path::PathBuf;
use std::sync::Mutex;
use tokio::process::Command;

/// Reverts via `git`, rooted at a workspace directory.
pub struct GitCheckpoint {
    workspace: PathBuf,
    /// Untracked files present at snapshot time. On restore, untracked files
    /// NOT in this set were created during execute and are removed.
    /// ponytail: assumes one snapshot→restore per instance (one harness run),
    /// which is exactly how the harness uses it.
    untracked_before: Mutex<Vec<String>>,
}

impl GitCheckpoint {
    #[must_use]
    pub fn new(workspace: impl Into<PathBuf>) -> Self {
        Self {
            workspace: workspace.into(),
            untracked_before: Mutex::new(Vec::new()),
        }
    }

    /// Runs a git subcommand, returning trimmed stdout. A non-zero exit is an
    /// error carrying git's stderr.
    async fn git(&self, args: &[&str]) -> Result<String, RegentError> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.workspace)
            .output()
            .await
            .map_err(|e| ckpt_err(format!("git {}: {e}", args.join(" "))))?;
        if !output.status.success() {
            return Err(ckpt_err(format!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    async fn is_git_repo(&self) -> bool {
        self.git(&["rev-parse", "--is-inside-work-tree"])
            .await
            .map(|out| out == "true")
            .unwrap_or(false)
    }

    /// Currently-untracked, non-ignored files (one per line).
    async fn untracked(&self) -> Result<Vec<String>, RegentError> {
        let out = self
            .git(&["ls-files", "--others", "--exclude-standard"])
            .await?;
        Ok(out
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect())
    }
}

fn ckpt_err(message: impl Into<String>) -> RegentError {
    RegentError::Tool {
        tool: "checkpoint".into(),
        message: message.into(),
    }
}

#[async_trait]
impl Checkpoint for GitCheckpoint {
    async fn snapshot(&self) -> Result<Option<String>, RegentError> {
        if !self.is_git_repo().await {
            return Ok(None);
        }
        let untracked = self.untracked().await?;
        // `stash create` captures tracked changes as a dangling commit WITHOUT
        // touching the working tree; empty when the tree is clean → use HEAD.
        let stash = self
            .git(&["stash", "create", "regent-code checkpoint"])
            .await?;
        let base = if stash.is_empty() {
            self.git(&["rev-parse", "HEAD"]).await?
        } else {
            stash
        };
        *self.untracked_before.lock().expect("checkpoint mutex") = untracked;
        Ok(Some(base))
    }

    async fn restore(&self, id: &str) -> Result<(), RegentError> {
        // Restore tracked files (modified + deleted) to the snapshot state.
        self.git(&["checkout", id, "--", "."]).await?;
        // Remove files the execute phase newly created (untracked now, but not
        // at snapshot); pre-existing untracked work is left untouched.
        let before = self
            .untracked_before
            .lock()
            .expect("checkpoint mutex")
            .clone();
        for path in self.untracked().await? {
            if !before.contains(&path) {
                let full = self.workspace.join(&path);
                if full.is_file() {
                    std::fs::remove_file(&full).map_err(|e| ckpt_err(e.to_string()))?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs a setup git command synchronously; returns false if git is missing
    /// or the command fails (so the test self-skips in a gitless environment).
    fn setup(dir: &std::path::Path, args: &[&str]) -> bool {
        std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[tokio::test]
    async fn restore_rewinds_edits_and_removes_new_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Self-skip when git isn't available / can't be configured.
        if !setup(root, &["init", "-q"])
            || !setup(root, &["config", "user.email", "t@t.t"])
            || !setup(root, &["config", "user.name", "t"])
            || !setup(root, &["config", "commit.gpgsign", "false"])
        {
            return;
        }
        std::fs::write(root.join("a.txt"), "original").unwrap();
        assert!(setup(root, &["add", "-A"]));
        assert!(setup(root, &["commit", "-q", "-m", "init"]));

        let ckpt = GitCheckpoint::new(root);
        let id = ckpt.snapshot().await.unwrap().expect("inside a git repo");

        // Simulate an execute phase: modify a tracked file + create a new one.
        std::fs::write(root.join("a.txt"), "clobbered").unwrap();
        std::fs::write(root.join("b.txt"), "new file").unwrap();

        ckpt.restore(&id).await.unwrap();

        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).unwrap(),
            "original"
        );
        assert!(
            !root.join("b.txt").exists(),
            "a newly-created file is removed on revert"
        );
    }

    #[tokio::test]
    async fn snapshot_outside_git_degrades_to_none() {
        let dir = tempfile::tempdir().unwrap();
        let ckpt = GitCheckpoint::new(dir.path());
        assert!(ckpt.snapshot().await.unwrap().is_none());
    }
}
