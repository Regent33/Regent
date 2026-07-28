//! Path-jail behaviour for `ToolContext::resolve`. Split from `entities.rs`
//! (file-size rule); same module tree via #[path].

use super::*;

use crate::domain::contracts::DenyAll;

fn ctx_sandboxed(root: &Path) -> ToolContext {
    ToolContext::new_sandboxed(root.to_path_buf(), root.to_path_buf(), Arc::new(DenyAll))
}

#[test]
fn unsandboxed_resolve_is_unrestricted() {
    let cwd = std::env::temp_dir();
    let ctx = ToolContext::new(cwd.clone(), Arc::new(DenyAll));
    assert!(!ctx.is_sandboxed());
    // A relative path joins to cwd; resolution never errors without a jail.
    assert_eq!(ctx.resolve("a/b.txt").unwrap(), cwd.join("a/b.txt"));
}

#[test]
fn sandbox_allows_paths_inside_root_and_rejects_escapes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let ctx = ctx_sandboxed(root);
    assert!(ctx.is_sandboxed());

    // Inside the root (existing dir + a not-yet-created file) is allowed.
    let inside = ctx.resolve("sub/new.txt").expect("inside root");
    assert!(inside.starts_with(root.canonicalize().unwrap()));

    // `..` traversal is rejected (platform-independent).
    assert!(ctx.resolve("../escape.txt").is_err());
    assert!(ctx.resolve("sub/../../escape.txt").is_err());

    // An absolute path outside the root is rejected (built from the root's
    // parent so it's genuinely absolute on every platform).
    let outside = root.parent().unwrap().join("outside.txt");
    assert!(ctx.resolve(outside.to_str().unwrap()).is_err());
}

#[test]
fn allow_subtree_widens_the_jail_to_exactly_that_subtree() {
    let jail = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let artifacts = home.path().join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let ctx = ctx_sandboxed(jail.path()).allow_subtree(artifacts.clone());

    // The jail and the extra subtree are both writable…
    assert!(ctx.resolve("inside.txt").is_ok());
    let shot = artifacts.join("shot.png");
    assert!(ctx.resolve(shot.to_str().unwrap()).is_ok());
    // …but the subtree's PARENT (where .env/state.db live) stays out.
    let env = home.path().join(".env");
    assert!(ctx.resolve(env.to_str().unwrap()).is_err());

    // allow_subtree on an unsandboxed context stays unrestricted.
    let open =
        ToolContext::new(jail.path().to_path_buf(), Arc::new(DenyAll)).allow_subtree(artifacts);
    assert!(!open.is_sandboxed());
}
