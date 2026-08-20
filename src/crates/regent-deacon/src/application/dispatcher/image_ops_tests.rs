//! Unit tests for `image_ops` (extracted for the file-size rule; same module
//! tree via #[path] — `use super::*` still sees the parent).

use super::{MAX_IMAGE_BYTES, read_image};
use std::path::PathBuf;

/// Smallest thing that reads as a PNG on disk; content is never parsed here.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\nDATA";

#[test]
fn inlines_a_small_image_as_a_data_uri() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("shot.png"), PNG).unwrap();
    let roots = vec![root.path().to_path_buf()];

    let got = read_image(&roots, "shot.png").unwrap();
    assert_eq!(got["mime"], "image/png");
    let uri = got["data_uri"].as_str().unwrap();
    assert!(uri.starts_with("data:image/png;base64,"), "{uri}");
    assert!(uri.len() > "data:image/png;base64,".len());
}

/// The path the app actually sends for a model-emitted image: absolute.
/// `Path::join` returns it unchanged, so it takes the same within-root gate.
#[test]
fn an_absolute_path_inside_a_root_resolves() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("shot.png");
    std::fs::write(&file, PNG).unwrap();
    let roots = vec![root.path().to_path_buf()];

    let got = read_image(&roots, &file.display().to_string()).unwrap();
    assert_eq!(got["mime"], "image/png");
}

#[test]
fn a_later_root_is_tried_when_the_first_misses() {
    let miss = tempfile::tempdir().unwrap();
    let hit = tempfile::tempdir().unwrap();
    std::fs::write(hit.path().join("shot.png"), PNG).unwrap();
    let roots = vec![miss.path().to_path_buf(), hit.path().to_path_buf()];

    assert!(read_image(&roots, "shot.png").is_ok());
}

#[test]
fn rejects_traversal_out_of_every_root() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let secret = outside.path().join("secret.png");
    std::fs::write(&secret, PNG).unwrap();
    let roots = vec![root.path().to_path_buf()];

    // Relative climb…
    let rel = format!(
        "../{}/secret.png",
        outside.path().file_name().unwrap().to_str().unwrap()
    );
    assert!(read_image(&roots, &rel).is_err());
    // …and the absolute form of the same file.
    assert!(read_image(&roots, &secret.display().to_string()).is_err());
}

#[test]
fn rejects_a_path_that_is_not_an_image() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("secrets.env"), b"KEY=1").unwrap();
    let roots = vec![root.path().to_path_buf()];

    // Never a general file-read oracle: the extension gate runs before the
    // path is even resolved.
    assert!(read_image(&roots, "secrets.env").is_err());
}

#[test]
fn rejects_an_image_over_the_cap() {
    let root = tempfile::tempdir().unwrap();
    let big = vec![0u8; usize::try_from(MAX_IMAGE_BYTES).unwrap() + 1];
    std::fs::write(root.path().join("huge.png"), &big).unwrap();
    let roots = vec![root.path().to_path_buf()];

    let error = read_image(&roots, "huge.png").unwrap_err();
    assert!(error.contains("exceeds"), "{error}");
}

#[test]
fn a_missing_file_is_an_error_not_a_panic() {
    let root = tempfile::tempdir().unwrap();
    let roots: Vec<PathBuf> = vec![root.path().to_path_buf()];

    assert!(read_image(&roots, "ghost.png").is_err());
    assert!(read_image(&[], "shot.png").is_err());
}
