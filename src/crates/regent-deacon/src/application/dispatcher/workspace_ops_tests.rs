//! Unit tests for `workspace_ops` — the pure root-scoped functions behind the
//! `workspace.*` RPCs (the dispatcher methods themselves stay thin, same
//! convention as `artifacts_ops_tests.rs`).

use super::*;

/// A temp workspace with a small tree inside it.
fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::write(root.join("README.md"), "# hi\n").unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(root.join("node_modules/pkg/index.js"), "x\n").unwrap();
    std::fs::write(root.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    dir
}

fn names(value: &Value) -> Vec<String> {
    value["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn tree_lists_directories_before_files_and_hides_noise() {
    let dir = workspace();
    let listing = tree_at(dir.path(), "").unwrap();
    let listed = names(&listing);
    assert!(listed.contains(&"src".to_owned()));
    assert!(listed.contains(&"README.md".to_owned()));
    // Build/VCS noise never reaches the panel.
    assert!(
        !listed.contains(&"node_modules".to_owned()),
        "got {listed:?}"
    );
    assert!(!listed.contains(&".git".to_owned()), "got {listed:?}");
    // Directories sort ahead of files.
    assert_eq!(listed.first().unwrap(), "src");
}

#[test]
fn tree_descends_into_a_subdirectory() {
    let dir = workspace();
    let listing = tree_at(dir.path(), "src").unwrap();
    assert_eq!(names(&listing), vec!["main.rs".to_owned()]);
}

#[test]
fn tree_rejects_a_path_escaping_the_workspace() {
    let dir = workspace();
    assert!(tree_at(dir.path(), "../..").is_err());
}

#[test]
fn tree_of_a_file_is_an_error_not_a_listing() {
    let dir = workspace();
    assert!(tree_at(dir.path(), "README.md").is_err());
}

#[test]
fn read_returns_text_and_a_revision_token() {
    let dir = workspace();
    let value = read_file_at(dir.path(), "src/main.rs").unwrap();
    assert_eq!(value["text"].as_str().unwrap(), "fn main() {}\n");
    assert!(
        value["rev"].as_str().is_some_and(|r| !r.is_empty()),
        "a revision token is required for the write-back check"
    );
}

#[test]
fn read_rejects_escape_missing_and_directories() {
    let dir = workspace();
    assert!(read_file_at(dir.path(), "../secrets").is_err());
    assert!(read_file_at(dir.path(), "nope.txt").is_err());
    assert!(
        read_file_at(dir.path(), "src").is_err(),
        "a dir is not a file"
    );
}

/// Binary content must NOT be lossily decoded: this path is read-then-save, so
/// replacement characters would be written back and corrupt the file on disk.
#[test]
fn read_of_a_non_utf8_file_reports_binary_instead_of_mangling_it() {
    let dir = workspace();
    std::fs::write(dir.path().join("logo.png"), [0xffu8, 0xd8, 0x00, 0x9f]).unwrap();
    let value = read_file_at(dir.path(), "logo.png").unwrap();
    assert_eq!(value["binary"], json!(true));
    assert!(value.get("text").is_none(), "binary must carry no text");
}

#[test]
fn write_replaces_the_file_when_the_revision_matches() {
    let dir = workspace();
    let before = read_file_at(dir.path(), "src/main.rs").unwrap();
    let rev = before["rev"].as_str().unwrap().to_owned();

    let result = write_file_at(dir.path(), "src/main.rs", "fn main() { todo!() }\n", &rev).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("src/main.rs")).unwrap(),
        "fn main() { todo!() }\n"
    );
    // The new token lets the editor keep saving without a re-read.
    assert!(result["rev"].as_str().is_some_and(|r| r != rev));
}

/// The lost-update guard: an editor buffer opened before a code task ran, then
/// saved after it finished, must NOT silently clobber the agent's edit.
#[test]
fn write_refuses_when_the_file_changed_since_it_was_read() {
    let dir = workspace();
    let stale = read_file_at(dir.path(), "src/main.rs").unwrap()["rev"]
        .as_str()
        .unwrap()
        .to_owned();

    // Something else (the agent) rewrites the file in the meantime.
    std::thread::sleep(std::time::Duration::from_millis(10));
    std::fs::write(dir.path().join("src/main.rs"), "fn main() { agent(); }\n").unwrap();

    let err =
        write_file_at(dir.path(), "src/main.rs", "stale editor buffer\n", &stale).unwrap_err();
    assert!(
        err.to_lowercase().contains("changed on disk"),
        "the refusal must say why, got: {err}"
    );
    // And the agent's version survives untouched.
    assert_eq!(
        std::fs::read_to_string(dir.path().join("src/main.rs")).unwrap(),
        "fn main() { agent(); }\n"
    );
}

#[test]
fn write_rejects_escape_and_missing_files() {
    let dir = workspace();
    assert!(write_file_at(dir.path(), "../evil.txt", "x", "").is_err());
    // v1 edits existing files only — no create-by-save.
    assert!(write_file_at(dir.path(), "brand-new.txt", "x", "").is_err());
}

#[test]
fn create_makes_an_empty_file_inside_the_workspace() {
    let dir = workspace();
    let value = create_at(dir.path(), "src/new.rs", "file").unwrap();
    assert_eq!(value["path"], json!("src/new.rs"));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("src/new.rs")).unwrap(),
        ""
    );
}

#[test]
fn create_makes_a_directory() {
    let dir = workspace();
    create_at(dir.path(), "src/nested/deep", "dir").unwrap();
    assert!(dir.path().join("src/nested/deep").is_dir());
}

/// Containment still holds for paths that do NOT exist yet — the existing
/// canonicalize-based check can't be used here, so this is its own guard.
#[test]
fn create_refuses_to_escape_the_workspace() {
    let dir = workspace();
    assert!(create_at(dir.path(), "../escaped.txt", "file").is_err());
    assert!(create_at(dir.path(), "src/../../escaped.txt", "file").is_err());
}

#[test]
fn create_refuses_to_clobber_something_that_already_exists() {
    let dir = workspace();
    let err = create_at(dir.path(), "README.md", "file").unwrap_err();
    assert!(err.contains("already exists"), "got: {err}");
}

/// Creating inside a folder that isn't there yet would silently invent the
/// whole chain for a FILE — refuse and let the user make the folder first.
#[test]
fn create_of_a_file_requires_its_parent_to_exist() {
    let dir = workspace();
    assert!(create_at(dir.path(), "nope/deeper/file.txt", "file").is_err());
}

#[test]
fn create_rejects_an_empty_or_dotted_name() {
    let dir = workspace();
    assert!(create_at(dir.path(), "", "file").is_err());
    assert!(create_at(dir.path(), "   ", "file").is_err());
    assert!(create_at(dir.path(), "src/..", "dir").is_err());
}
