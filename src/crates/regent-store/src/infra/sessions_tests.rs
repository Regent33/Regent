//! Unit tests for `sessions` — currently the per-session workspace root a
//! Desktop coding session opens (extracted for the file-size rule).

use crate::infra::db::Store;
use regent_kernel::SessionId;

fn store() -> Store {
    Store::open_in_memory().unwrap()
}

fn new_session(store: &Store) -> SessionId {
    let id = SessionId::generate();
    store.create_session(&id, "desktop", None, None, None).unwrap();
    id
}

/// The override survives a round trip, so resuming after an app restart
/// re-opens the same folder instead of silently falling back to the sandbox.
#[test]
fn workspace_round_trips() {
    let store = store();
    let id = new_session(&store);
    let root = r"D:\projects\my-app";
    store.set_session_workspace(&id, root).unwrap();
    assert_eq!(store.session_workspace(&id).unwrap(), Some(root.to_owned()));
}

/// A session that never opened a folder reports None — that is what makes the
/// resume path fall back to the deacon's own cwd (every CLI/platform session).
#[test]
fn a_session_without_a_workspace_reports_none() {
    let store = store();
    let id = new_session(&store);
    assert_eq!(store.session_workspace(&id).unwrap(), None);
}

/// An unknown id is None, not an error — `resume_session` reads this before it
/// can know whether the row exists, and a missing row must not abort a resume.
#[test]
fn an_unknown_session_reports_none_rather_than_erroring() {
    let store = store();
    let missing = SessionId::generate();
    assert_eq!(store.session_workspace(&missing).unwrap(), None);
}

/// Re-opening a different folder for the same session replaces the old root
/// (no history, no append) — the workspace is a single current value.
#[test]
fn setting_a_second_workspace_replaces_the_first() {
    let store = store();
    let id = new_session(&store);
    store.set_session_workspace(&id, "/first").unwrap();
    store.set_session_workspace(&id, "/second").unwrap();
    assert_eq!(store.session_workspace(&id).unwrap(), Some("/second".to_owned()));
}
