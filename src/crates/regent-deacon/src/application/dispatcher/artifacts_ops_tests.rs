//! Unit tests for `artifacts_ops` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::{classify_kind, get_artifact, guess_mime, list_artifacts};

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
