//! Rebinding a live session onto another folder.
//!
//! Reported 2026-07-29: picking a repo mid-conversation started a NEW chat and
//! navigated away from the one the user was in. The workspace was fixed at
//! birth, so the Desktop button had nowhere to put the folder.
//!
//! The security property a rebind depends on — that opening a real folder turns
//! the jail ON, so the context must be RECOMPUTED rather than have its root
//! edited — is already pinned next door by
//! `session_sandbox::a_session_that_opened_a_workspace_is_always_sandboxed`.
//! `rebind_workspace` goes through the same `tool_context` constructor birth
//! uses precisely so that test keeps covering it. What is new here, and what
//! this file covers, is the refusal path.

use super::super::workspace_rebind::resolve_workspace_root;

/// A path that is not a usable directory must be refused BEFORE the session is
/// touched: a typo cannot be allowed to leave a live conversation pointed at
/// somewhere that does not exist.
#[test]
fn a_missing_folder_is_refused_with_the_real_reason() {
    let dir = tempfile::tempdir().unwrap();

    let err = resolve_workspace_root(&dir.path().join("no-such-folder"))
        .expect_err("a missing directory must be an error");

    let message = format!("{err}");
    assert!(
        message.contains("cannot open"),
        "the caller needs the path failure, not a misleading one: {message}"
    );
}

/// A file is canonicalizable but is not a workspace.
#[test]
fn a_file_is_refused_even_though_it_resolves() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("README.md");
    std::fs::write(&file, "not a folder").unwrap();

    let err = resolve_workspace_root(&file).expect_err("a file is not a workspace");

    assert!(format!("{err}").contains("not a directory"), "got: {err}");
}

/// The happy path resolves to a real directory the tools can be jailed to.
#[test]
fn a_real_folder_resolves() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    std::fs::create_dir_all(&project).unwrap();

    let resolved = resolve_workspace_root(&project).expect("a real folder resolves");

    assert!(resolved.is_dir());
}
