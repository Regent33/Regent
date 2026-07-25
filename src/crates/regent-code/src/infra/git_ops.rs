//! Read/write git operations for a workspace: status, diff, commit, push.
//! Stateless free functions (unlike `GitCheckpoint`, which carries snapshot
//! state), each rooted at a caller-supplied directory.
//!
//! Shells out to the `git` CLI rather than linking a git library, matching
//! `checkpoint.rs` — and deliberately so for `push`, which then reuses the
//! user's own credential helper, SSH agent, and git config for free.
//!
//! Failures carry git's OWN stderr verbatim. "no upstream configured" tells
//! the user exactly which command fixes it; paraphrasing would hide that.

use regent_kernel::RegentError;
use std::path::Path;
use tokio::process::Command;

/// One changed path in a working tree.
pub struct GitStatusEntry {
    pub path: String,
    /// The raw two-letter porcelain code (e.g. " M", "??", "A ").
    pub status: String,
    /// Whether the index (not just the worktree) carries the change.
    pub staged: bool,
}

/// A working tree's state: whether it is a repo at all, where it points, and
/// what has changed.
pub struct GitStatus {
    pub is_repo: bool,
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub entries: Vec<GitStatusEntry>,
}

impl GitStatus {
    /// A tree with nothing to commit — the panel disables Commit on this.
    #[must_use]
    pub fn dirty(&self) -> bool {
        !self.entries.is_empty()
    }
}

fn git_err(message: impl Into<String>) -> RegentError {
    RegentError::Tool {
        tool: "git".into(),
        message: message.into(),
    }
}

/// Run a git subcommand, returning stdout with only TRAILING whitespace
/// removed; a non-zero exit is an error carrying git's stderr (falling back to
/// stdout, which is where some git commands — notably `commit` on a clean tree
/// — put their explanation).
///
/// `trim_end`, never `trim`: `status --porcelain` encodes index/worktree state
/// in columns 0-1, so a modified-unstaged file starts with a SPACE (" M a.txt").
/// Trimming the front eats it and shifts every parsed path by one character.
async fn git(cwd: &Path, args: &[&str]) -> Result<String, RegentError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .map_err(|e| git_err(format!("git {}: {e}", args.join(" "))))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let detail = if stderr.is_empty() {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        } else {
            stderr
        };
        return Err(git_err(format!("git {} failed: {detail}", args.join(" "))));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_owned())
}

async fn is_repo(cwd: &Path) -> bool {
    git(cwd, &["rev-parse", "--is-inside-work-tree"])
        .await
        .map(|out| out == "true")
        .unwrap_or(false)
}

/// Parse `git status --porcelain` (v1). Columns 0/1 are index/worktree state,
/// the path starts at column 3.
fn parse_porcelain(out: &str) -> Vec<GitStatusEntry> {
    out.lines()
        .filter(|line| line.len() > 3)
        .map(|line| {
            let code = &line[..2];
            // A rename reads "R  old -> new"; the new path is what the user edits.
            let raw = &line[3..];
            let path = raw.rsplit(" -> ").next().unwrap_or(raw);
            GitStatusEntry {
                path: path.trim_matches('"').to_owned(),
                status: code.to_owned(),
                staged: !matches!(code.as_bytes()[0], b' ' | b'?'),
            }
        })
        .collect()
}

/// Ahead/behind counts from `rev-list --left-right --count @{u}...HEAD`, whose
/// output is "<behind>\t<ahead>". Absent upstream → (0, 0).
fn parse_ahead_behind(out: &str) -> (u32, u32) {
    let mut parts = out.split_whitespace();
    let behind = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let ahead = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    (ahead, behind)
}

/// The working tree's state. A non-repo is reported as `is_repo: false` rather
/// than an error — "this folder isn't a repo yet" is a normal thing for the
/// panel to display, not a failure.
pub async fn git_status(cwd: &Path) -> Result<GitStatus, RegentError> {
    if !is_repo(cwd).await {
        return Ok(GitStatus {
            is_repo: false,
            branch: None,
            upstream: None,
            ahead: 0,
            behind: 0,
            entries: Vec::new(),
        });
    }
    let branch = git(cwd, &["branch", "--show-current"])
        .await
        .ok()
        .filter(|b| !b.is_empty());
    // No upstream is normal (a fresh branch), so a failure here is None, not Err.
    let upstream = git(cwd, &["rev-parse", "--abbrev-ref", "@{u}"]).await.ok();
    let (ahead, behind) = match upstream {
        Some(_) => git(cwd, &["rev-list", "--left-right", "--count", "@{u}...HEAD"])
            .await
            .map(|out| parse_ahead_behind(&out))
            .unwrap_or((0, 0)),
        None => (0, 0),
    };
    let entries = parse_porcelain(&git(cwd, &["status", "--porcelain"]).await?);
    Ok(GitStatus {
        is_repo: true,
        branch,
        upstream,
        ahead,
        behind,
        entries,
    })
}

/// Stage everything and commit. Returns the new short sha. A clean tree, a
/// missing identity, and a non-repo all surface git's own wording.
pub async fn git_commit(cwd: &Path, message: &str) -> Result<String, RegentError> {
    git(cwd, &["add", "-A"]).await?;
    git(cwd, &["commit", "-m", message]).await?;
    git(cwd, &["rev-parse", "--short", "HEAD"]).await
}

/// Push the current branch to its configured upstream. No implicit
/// `--set-upstream`: on a branch without one, git's error names the exact
/// command to run, and silently inventing a remote/branch pairing is the kind
/// of guess that pushes work somewhere the user didn't intend.
pub async fn git_push(cwd: &Path) -> Result<String, RegentError> {
    git(cwd, &["push"]).await
}

/// The working tree's diff against HEAD (staged + unstaged), capped so a huge
/// change can't blow up a review prompt. `None` when git is unavailable or this
/// isn't a repo.
// ponytail: untracked files don't show in `git diff HEAD` — reviews cover
// edits; add `--intent-to-add` plumbing if new-file review ever matters.
pub async fn git_diff(cwd: &Path) -> Option<String> {
    const DIFF_CAP_CHARS: usize = 60_000;
    let mut diff = git(cwd, &["diff", "HEAD"]).await.ok()?;
    if diff.chars().count() > DIFF_CAP_CHARS {
        diff = diff.chars().take(DIFF_CAP_CHARS).collect();
        diff.push_str("\n[diff truncated for review]");
    }
    Some(diff)
}

#[cfg(test)]
#[path = "git_ops_tests.rs"]
mod tests;
