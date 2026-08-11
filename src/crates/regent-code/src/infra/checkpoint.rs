//! Git-backed working-tree checkpoint for revert-to-last-green. Snapshot
//! captures the pre-execute tree state (preserving any pre-existing uncommitted
//! work); restore rewinds tracked modifications/deletions and removes files the
//! execute phase newly created. Outside a git repo, snapshot returns `None` so
//! the harness degrades to report-only rather than guessing.

use crate::application::Checkpoint;
use async_trait::async_trait;
use regent_kernel::RegentError;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
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

    /// Runs a git subcommand, returning stdout with only its trailing newline
    /// removed. NOT `.trim()`: that strips leading whitespace from the whole
    /// buffer, and git sorts `0x20` before nearly every printable character, so
    /// an untracked file named " leading.txt" is deterministically the first
    /// record and lost its space — re-opening, for whitespace-prefixed names,
    /// exactly the content-blind hole `-z` was added to close.
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
        Ok(String::from_utf8_lossy(&output.stdout)
            .trim_end_matches(['\n', '\r'])
            .to_owned())
    }

    async fn is_git_repo(&self) -> bool {
        self.git(&["rev-parse", "--is-inside-work-tree"])
            .await
            .map(|out| out == "true")
            .unwrap_or(false)
    }

    /// Currently-untracked, non-ignored files.
    ///
    /// `-z` for NUL separation, because git's default `core.quotePath` C-quotes
    /// any path with a non-ASCII byte — `café.rs` comes back as the literal
    /// `"caf\303\251.rs"`. Joining THAT to the workspace resolves to nothing,
    /// which silently broke both callers: `fingerprint` hashed the unreadable
    /// marker instead of the file's contents (re-opening the very blind spot it
    /// was written to close), and `restore` failed to remove such a file.
    /// `-z` also makes a path containing a newline unambiguous.
    async fn untracked(&self) -> Result<Vec<String>, RegentError> {
        let out = self
            .git(&["ls-files", "--others", "--exclude-standard", "-z"])
            .await?;
        Ok(out
            .split('\0')
            .filter(|p| !p.is_empty())
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
    /// Tracked changes, plus the CONTENT of untracked files.
    ///
    /// Status and `diff HEAD` between them cover tracked work, but neither can
    /// see inside an untracked file: `?? path` is printed identically whatever
    /// the file holds, and `diff HEAD` does not consider untracked paths at
    /// all. That gap is the common `regent-code` shape — the execute turn
    /// CREATES a file (never staged), verify goes red on an error inside it,
    /// and the fix turn edits that same new file. A fingerprint blind to the
    /// edit reports "nothing changed", the gate is skipped, the stale red
    /// stands, and `restore` then deletes the very file the fix corrected.
    /// So untracked content is hashed, not just listed.
    async fn fingerprint(&self) -> Option<String> {
        if !self.is_git_repo().await {
            return None;
        }
        let status = self.git(&["status", "--porcelain", "-uall"]).await.ok()?;
        let diff = self.git(&["diff", "HEAD"]).await.unwrap_or_default();
        let mut hasher = DefaultHasher::new();
        // `--exclude-standard` (see `untracked`) already drops ignored paths,
        // so this reads new source files, not build output.
        for path in self.untracked().await.unwrap_or_default() {
            path.hash(&mut hasher);
            match tokio::fs::read(self.workspace.join(&path)).await {
                Ok(bytes) => bytes.hash(&mut hasher),
                // Unreadable (racing delete, permissions) — record the failure
                // itself so it still differs from a readable file.
                Err(_) => 0u8.hash(&mut hasher),
            }
        }
        Some(format!("{status}\u{1}{diff}\u{1}{:x}", hasher.finish()))
    }

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

    /// The fingerprint decides whether the harness re-runs the verify gate. A
    /// blind spot here is not a missed optimisation — the stale RED stands and
    /// `restore` then deletes the file the fix turn just corrected. `git status`
    /// prints `?? path` whatever the file holds and `git diff HEAD` ignores
    /// untracked paths entirely, so this is exactly the case that got missed.
    #[tokio::test]
    async fn fingerprint_sees_a_content_edit_to_an_untracked_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
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

        // The execute turn creates a new file — never staged, so untracked.
        std::fs::write(root.join("new_mod.rs"), "fn broken( {").unwrap();
        let after_create = ckpt.fingerprint().await.expect("inside a git repo");

        // The fix turn edits that same untracked file. Nothing about its
        // git STATUS changes; only its contents do.
        std::fs::write(root.join("new_mod.rs"), "fn fixed() {}").unwrap();
        let after_fix = ckpt.fingerprint().await.unwrap();

        assert_ne!(
            after_create, after_fix,
            "an edit to an untracked file must move the fingerprint, or the              harness skips the gate and reverts a tree the fix just made green"
        );

        // And an untouched tree must still compare equal, or the optimisation
        // never fires at all.
        assert_eq!(after_fix, ckpt.fingerprint().await.unwrap());
    }

    /// git C-quotes non-ASCII paths by default, so the untracked list came back
    /// as the literal `"caf\303\251.rs"` — a path that resolves to nothing. The
    /// fingerprint then hashed its unreadable marker instead of the contents,
    /// which is exactly the blind spot it exists to close, narrowed to files
    /// with accented or CJK names.
    #[tokio::test]
    async fn fingerprint_sees_an_edit_to_an_untracked_non_ascii_name() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
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
        std::fs::write(root.join("café.rs"), "fn broken( {").unwrap();
        let before = ckpt.fingerprint().await.expect("inside a git repo");
        std::fs::write(root.join("café.rs"), "fn fixed() {}").unwrap();

        assert_ne!(
            before,
            ckpt.fingerprint().await.unwrap(),
            "a C-quoted path must still resolve, or the gate is blind again"
        );
    }

    /// git sorts 0x20 first, so a file named " leading.txt" is deterministically
    /// the FIRST record — and a `.trim()` over the whole stdout buffer ate that
    /// space, leaving a path that resolves to nothing. Same blind spot as the
    /// C-quoting one, just moved next door.
    #[tokio::test]
    async fn fingerprint_sees_an_edit_to_a_path_beginning_with_whitespace() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
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
        std::fs::write(root.join(" leading.txt"), "v1").unwrap();
        let before = ckpt.fingerprint().await.expect("inside a git repo");
        std::fs::write(root.join(" leading.txt"), "v2 entirely different").unwrap();

        assert_ne!(
            before,
            ckpt.fingerprint().await.unwrap(),
            "a leading space must survive, or the gate is blind for that file"
        );
    }

    #[tokio::test]
    async fn fingerprint_outside_git_is_none_so_the_gate_always_runs() {
        let dir = tempfile::tempdir().unwrap();
        assert!(GitCheckpoint::new(dir.path()).fingerprint().await.is_none());
    }
}
