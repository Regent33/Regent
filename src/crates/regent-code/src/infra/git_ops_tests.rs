//! Unit tests for `git_ops` — real `git` subprocesses in a temp repo, same
//! shape as `checkpoint.rs`'s tests (and the same self-skip when git is
//! unavailable, so CI without git stays green).

use super::*;
use std::path::Path;

/// Run a setup git command, reporting whether it succeeded.
fn setup(dir: &Path, args: &[&str]) -> bool {
    std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// A temp git repo with one commit, or `None` when git can't be used here.
fn repo_with_commit(root: &Path) -> bool {
    if !setup(root, &["init", "-q"])
        || !setup(root, &["config", "user.email", "t@t.t"])
        || !setup(root, &["config", "user.name", "t"])
        || !setup(root, &["config", "commit.gpgsign", "false"])
    {
        return false;
    }
    std::fs::write(root.join("a.txt"), "original\n").unwrap();
    setup(root, &["add", "-A"]) && setup(root, &["commit", "-q", "-m", "init"])
}

#[tokio::test]
async fn status_outside_a_repo_reports_not_a_repo_rather_than_erroring() {
    let dir = tempfile::tempdir().unwrap();
    let status = git_status(dir.path()).await.unwrap();
    assert!(!status.is_repo, "a plain directory is not a repo");
    assert!(status.entries.is_empty());
    assert!(status.branch.is_none());
}

#[tokio::test]
async fn status_on_a_clean_repo_lists_no_entries() {
    let dir = tempfile::tempdir().unwrap();
    if !repo_with_commit(dir.path()) {
        return;
    }
    let status = git_status(dir.path()).await.unwrap();
    assert!(status.is_repo);
    assert!(status.entries.is_empty(), "clean tree has no entries");
    assert!(
        status.branch.is_some(),
        "a committed repo reports its branch"
    );
    assert!(status.upstream.is_none(), "no remote configured yet");
}

#[tokio::test]
async fn status_reports_modified_and_untracked_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    if !repo_with_commit(root) {
        return;
    }
    std::fs::write(root.join("a.txt"), "edited\n").unwrap();
    std::fs::write(root.join("new.txt"), "fresh\n").unwrap();

    let status = git_status(root).await.unwrap();
    let paths: Vec<&str> = status.entries.iter().map(|e| e.path.as_str()).collect();
    assert!(paths.contains(&"a.txt"), "modified file listed: {paths:?}");
    assert!(
        paths.contains(&"new.txt"),
        "untracked file listed: {paths:?}"
    );
}

#[tokio::test]
async fn commit_stages_everything_and_returns_the_new_sha() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    if !repo_with_commit(root) {
        return;
    }
    std::fs::write(root.join("a.txt"), "edited\n").unwrap();
    std::fs::write(root.join("new.txt"), "fresh\n").unwrap();

    let sha = git_commit(root, "test: edit and add").await.unwrap();
    assert!(!sha.is_empty(), "a commit reports its sha");
    // Everything was staged, so the tree is clean afterwards.
    let status = git_status(root).await.unwrap();
    assert!(status.entries.is_empty(), "commit -A leaves a clean tree");
}

#[tokio::test]
async fn committing_a_clean_tree_fails_with_a_clear_message() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    if !repo_with_commit(root) {
        return;
    }
    let err = git_commit(root, "nothing to do").await.unwrap_err();
    let text = err.to_string().to_lowercase();
    assert!(
        text.contains("nothing to commit") || text.contains("no changes"),
        "a no-op commit must say so plainly, got: {text}"
    );
}

#[tokio::test]
async fn committing_outside_a_repo_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    assert!(git_commit(dir.path(), "msg").await.is_err());
}

#[tokio::test]
async fn push_without_an_upstream_surfaces_gits_own_message() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    if !repo_with_commit(root) {
        return;
    }
    let err = git_push(root).await.unwrap_err();
    // Verbatim git stderr — the fix ("git push -u origin <branch>") is in it,
    // and inventing our own wording would hide that.
    assert!(!err.to_string().is_empty());
}

#[tokio::test]
async fn push_to_a_configured_remote_delivers_the_commit() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("work");
    let remote = dir.path().join("remote.git");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&remote).unwrap();
    if !setup(&remote, &["init", "--bare", "-q"]) || !repo_with_commit(&root) {
        return;
    }
    let remote_arg = remote.display().to_string();
    if !setup(&root, &["remote", "add", "origin", &remote_arg]) {
        return;
    }
    // Determine the branch git actually created (main vs master varies).
    let status = git_status(&root).await.unwrap();
    let branch = status
        .branch
        .clone()
        .expect("a committed repo has a branch");
    if !setup(&root, &["push", "-q", "-u", "origin", &branch]) {
        return;
    }

    std::fs::write(root.join("a.txt"), "second\n").unwrap();
    git_commit(&root, "test: second").await.unwrap();
    git_push(&root).await.unwrap();

    // The bare remote now has two commits on that branch.
    let out = std::process::Command::new("git")
        .args(["rev-list", "--count", &branch])
        .current_dir(&remote)
        .output()
        .unwrap();
    let count = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    assert_eq!(count, "2", "the remote received the pushed commit");
}

#[tokio::test]
async fn status_reports_ahead_count_against_an_upstream() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("work");
    let remote = dir.path().join("remote.git");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&remote).unwrap();
    if !setup(&remote, &["init", "--bare", "-q"]) || !repo_with_commit(&root) {
        return;
    }
    let remote_arg = remote.display().to_string();
    if !setup(&root, &["remote", "add", "origin", &remote_arg]) {
        return;
    }
    let branch = git_status(&root).await.unwrap().branch.unwrap();
    if !setup(&root, &["push", "-q", "-u", "origin", &branch]) {
        return;
    }

    std::fs::write(root.join("a.txt"), "ahead\n").unwrap();
    git_commit(&root, "test: ahead by one").await.unwrap();

    let status = git_status(&root).await.unwrap();
    assert!(status.upstream.is_some(), "upstream is reported once set");
    assert_eq!(status.ahead, 1, "one unpushed commit");
    assert_eq!(status.behind, 0);
}

#[tokio::test]
async fn diff_returns_none_outside_a_repo_and_text_inside() {
    let dir = tempfile::tempdir().unwrap();
    assert!(git_diff(dir.path()).await.is_none());

    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    if !repo_with_commit(&repo) {
        return;
    }
    std::fs::write(repo.join("a.txt"), "changed\n").unwrap();
    let diff = git_diff(&repo).await.expect("inside a repo");
    assert!(diff.contains("a.txt"), "diff names the changed file");
}
