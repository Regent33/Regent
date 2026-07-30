//! Unit tests for `artifacts_ops` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::{classify_kind, delete_artifact, get_artifact, guess_mime, list_artifacts};

#[test]
fn kind_and_mime_by_extension() {
    assert_eq!(classify_kind("brief.md"), "text");
    assert_eq!(classify_kind("dog.JPG"), "image");
    // A double extension classifies by its last segment.
    assert_eq!(classify_kind("CV.docx.pdf"), "other");
    assert_eq!(guess_mime("shot.png"), "image/png");
    assert_eq!(guess_mime("brief.md"), "text/markdown");
    assert_eq!(guess_mime("CV.docx.pdf"), "application/pdf");
    assert_eq!(guess_mime("mystery.zzz"), "application/octet-stream");
}

#[test]
fn list_reports_slugs_and_file_kinds() {
    let root = tempfile::tempdir().unwrap();
    let slug = root.path().join("ai-brief");
    std::fs::create_dir_all(&slug).unwrap();
    std::fs::write(slug.join("brief.md"), b"# hi").unwrap();
    std::fs::write(slug.join("shot.png"), b"\x89PNG\r\n\x1a\n").unwrap();
    std::fs::write(slug.join(".hidden"), b"skip").unwrap();

    let list = list_artifacts(root.path());
    let arr = list.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "ai-brief");
    assert!(arr[0]["created_at"].as_f64().unwrap() > 0.0);

    let files = arr[0]["files"].as_array().unwrap();
    // Dotfile skipped → the two real files remain.
    assert_eq!(files.len(), 2);
    let kinds: Vec<&str> = files.iter().map(|f| f["kind"].as_str().unwrap()).collect();
    assert!(kinds.contains(&"text"));
    assert!(kinds.contains(&"image"));
    let md = files.iter().find(|f| f["name"] == "brief.md").unwrap();
    assert_eq!(md["rel"], "ai-brief/brief.md");
    assert_eq!(md["bytes"].as_u64().unwrap(), 4);
}

/// A file loose at the root used to be skipped, so it existed on disk and
/// nowhere in the app. Every document written before per-session folders landed
/// there, and the owner could only find them in Explorer.
#[test]
fn a_loose_file_at_the_root_is_listed_and_openable() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("alice.pdf"), b"%PDF-1.4").unwrap();
    std::fs::create_dir_all(root.path().join("a-folder")).unwrap();
    std::fs::write(root.path().join(".hidden.pdf"), b"skip").unwrap();

    let list = list_artifacts(root.path());
    let arr = list.as_array().unwrap();
    let loose = arr
        .iter()
        .find(|entry| entry["name"] == "alice.pdf")
        .expect("the loose file must be listed");
    let files = loose["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    // A bare `rel` — what `artifacts.get`/`delete` resolve against the root.
    assert_eq!(files[0]["rel"], "alice.pdf");
    assert_eq!(files[0]["bytes"].as_u64().unwrap(), 8);

    // Still reachable and removable through the same two calls the UI uses.
    assert!(get_artifact(root.path(), "alice.pdf").is_ok());
    assert!(delete_artifact(root.path(), "alice.pdf").is_ok());
    assert!(!root.path().join("alice.pdf").exists());
    // The dotfile stayed skipped, and the folder is still its own entry.
    assert!(arr.iter().any(|e| e["name"] == "a-folder"));
    assert!(!arr.iter().any(|e| e["name"] == ".hidden.pdf"));
}

#[test]
fn empty_or_missing_root_is_empty_array() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(list_artifacts(root.path()), serde_json::json!([]));
    assert_eq!(
        list_artifacts(&root.path().join("does-not-exist")),
        serde_json::json!([])
    );
}

#[test]
fn get_returns_text_and_rejects_escape() {
    let root = tempfile::tempdir().unwrap();
    let slug = root.path().join("notes");
    std::fs::create_dir_all(&slug).unwrap();
    std::fs::write(slug.join("a.md"), b"hello").unwrap();

    let ok = get_artifact(root.path(), "notes/a.md").unwrap();
    assert_eq!(ok["kind"], "text");
    assert_eq!(ok["mime"], "text/markdown");
    assert_eq!(ok["text"], "hello");
    assert_eq!(ok["path"], "notes/a.md");
    assert!(ok["abs"].as_str().unwrap().ends_with("a.md"));
    assert!(ok.get("data_base64").is_none());

    // A traversal path that climbs out of the root is rejected.
    assert!(get_artifact(root.path(), "../escape.md").is_err());
}

#[test]
fn delete_removes_the_file_and_reports_ok() {
    let root = tempfile::tempdir().unwrap();
    let slug = root.path().join("notes");
    std::fs::create_dir_all(&slug).unwrap();
    let file = slug.join("a.md");
    std::fs::write(&file, b"hello").unwrap();

    assert!(delete_artifact(root.path(), "notes/a.md").is_ok());
    assert!(!file.exists());
}

#[test]
fn delete_rejects_a_path_that_escapes_the_root() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let victim = outside.path().join("victim.md");
    std::fs::write(&victim, b"do not touch").unwrap();

    assert!(delete_artifact(root.path(), "../victim.md").is_err());
    assert!(victim.exists());
}

#[test]
fn delete_of_a_missing_file_is_an_error_not_a_panic() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("notes")).unwrap();
    assert!(delete_artifact(root.path(), "notes/missing.md").is_err());
}

#[test]
fn delete_of_a_top_level_slug_removes_the_whole_folder() {
    let root = tempfile::tempdir().unwrap();
    let slug = root.path().join("notes");
    std::fs::create_dir_all(&slug).unwrap();
    std::fs::write(slug.join("a.md"), b"hello").unwrap();
    std::fs::write(slug.join("b.md"), b"world").unwrap();

    assert!(delete_artifact(root.path(), "notes").is_ok());
    assert!(!slug.exists());
}

#[test]
fn delete_refuses_a_nested_directory_below_a_slug() {
    let root = tempfile::tempdir().unwrap();
    let nested = root.path().join("notes").join("sub");
    std::fs::create_dir_all(&nested).unwrap();

    // Only a direct top-level slug may be removed as a folder — never a
    // directory nested inside one, which would need its own escape checks.
    assert!(delete_artifact(root.path(), "notes/sub").is_err());
    assert!(nested.exists());
}

#[test]
fn get_inlines_small_image_as_base64() {
    let root = tempfile::tempdir().unwrap();
    let slug = root.path().join("pics");
    std::fs::create_dir_all(&slug).unwrap();
    std::fs::write(slug.join("p.png"), b"\x89PNG\r\n\x1a\nDATA").unwrap();

    let got = get_artifact(root.path(), "pics/p.png").unwrap();
    assert_eq!(got["kind"], "image");
    assert_eq!(got["mime"], "image/png");
    assert!(got.get("text").is_none());
    assert!(!got["data_base64"].as_str().unwrap().is_empty());
}
