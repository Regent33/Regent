//! Unit tests for `git_ops`'s pure input validation. The git behavior itself
//! is tested in `regent_code::git_ops` against real repos; these cover only
//! what this layer adds — rejecting a workspace path before a session is built
//! around it.

use super::resolve_workspace_path;

#[test]
fn a_real_directory_resolves() {
    let dir = tempfile::tempdir().unwrap();
    let resolved = resolve_workspace_path(&dir.path().display().to_string()).unwrap();
    assert!(resolved.is_dir());
}

#[test]
fn surrounding_whitespace_is_tolerated() {
    let dir = tempfile::tempdir().unwrap();
    let padded = format!("  {}  ", dir.path().display());
    assert!(resolve_workspace_path(&padded).is_ok());
}

#[test]
fn an_empty_or_missing_path_is_rejected() {
    assert!(resolve_workspace_path("").is_err());
    assert!(resolve_workspace_path("   ").is_err());
    assert!(resolve_workspace_path("/no/such/folder/anywhere").is_err());
}

/// A file is not a workspace — catching it here beats letting every later
/// tree/read/write call fail against it.
#[test]
fn a_file_is_rejected_as_a_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    std::fs::write(&file, "x").unwrap();
    assert!(resolve_workspace_path(&file.display().to_string()).is_err());
}
