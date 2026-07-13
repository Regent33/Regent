//! Unit tests for `diagnostics` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

#[test]
fn patch_path_finds_update_and_add_headers() {
    let patch = json!({"patch": "*** Begin Patch\n*** Update File: src/lib.rs\n@@\n*** End Patch"});
    assert_eq!(patch_path(&patch).as_deref(), Some("src/lib.rs"));
    let add = json!({"patch": "*** Begin Patch\n*** Add File: a/b.py\n*** End Patch"});
    assert_eq!(patch_path(&add).as_deref(), Some("a/b.py"));
    assert_eq!(patch_path(&json!({"patch": "no headers"})), None);
}

#[test]
fn command_selection_respects_manifests() {
    let dir = tempfile::tempdir().unwrap();
    // No manifests at all → rust/ts checks unavailable, node/py still apply.
    let d = Diagnostics::detect(dir.path());
    assert_eq!(d.command_for("src/main.rs"), None);
    assert_eq!(d.command_for("app.ts"), None);
    assert!(d.command_for("app.js").is_some());
    assert!(d.command_for("app.py").is_some());
    assert_eq!(d.command_for("README.md"), None);

    std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
    std::fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();
    let d = Diagnostics::detect(dir.path());
    assert_eq!(
        d.command_for("src/main.rs").unwrap()[..2],
        ["cargo".to_owned(), "check".to_owned()]
    );
    assert_eq!(d.command_for("app.ts").unwrap()[0], "tsc");
}
