//! Unit tests for `models` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

fn spec(files: Vec<ModelFile>) -> ModelSpec {
    ModelSpec {
        kind: ModelKind::Asr,
        id: "qwen3-asr".into(),
        files,
    }
}

#[test]
fn model_dir_layout_is_kind_then_id() {
    let mgr = ModelManager::new("/models");
    assert_eq!(
        mgr.model_dir(ModelKind::Tts, "qwen3-tts"),
        Path::new("/models/tts/qwen3-tts")
    );
}

#[test]
fn sha256_hex_is_stable() {
    // Known SHA-256 of "abc".
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn ensure_downloads_then_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let mgr = ModelManager::new(tmp.path());
    let body = b"weights".to_vec();
    let s = spec(vec![ModelFile {
        name: "model.bin".into(),
        url: "https://example/model.bin".into(),
        sha256: sha256_hex(&body),
    }]);
    assert!(!mgr.is_present(&s));

    let calls = std::cell::Cell::new(0);
    let fetch = |_url: &str| {
        calls.set(calls.get() + 1);
        Ok(body.clone())
    };
    let dir = mgr.ensure(&s, fetch).unwrap();
    assert!(dir.join("model.bin").is_file());
    assert!(mgr.is_present(&s));
    assert_eq!(calls.get(), 1);

    // Second call: already present → no fetch.
    mgr.ensure(&s, fetch).unwrap();
    assert_eq!(calls.get(), 1);
}

#[test]
fn ensure_rejects_a_checksum_mismatch_without_storing() {
    let tmp = tempfile::tempdir().unwrap();
    let mgr = ModelManager::new(tmp.path());
    let s = spec(vec![ModelFile {
        name: "model.bin".into(),
        url: "https://example/model.bin".into(),
        sha256: sha256_hex(b"expected"),
    }]);
    let err = mgr.ensure(&s, |_| Ok(b"corrupted".to_vec())).unwrap_err();
    assert!(matches!(err, ManagerError::Checksum { .. }));
    assert!(!mgr.is_present(&s));
}

#[test]
fn ensure_rejects_path_traversal_in_name_or_id_before_fetching() {
    let tmp = tempfile::tempdir().unwrap();
    let mgr = ModelManager::new(tmp.path());

    // A traversal `name` is rejected and `fetch` never runs.
    let fetched = std::cell::Cell::new(false);
    let evil_name = spec(vec![ModelFile {
        name: "../../evil.bin".into(),
        url: "u".into(),
        sha256: String::new(),
    }]);
    let err = mgr
        .ensure(&evil_name, |_| {
            fetched.set(true);
            Ok(b"x".to_vec())
        })
        .unwrap_err();
    assert!(matches!(err, ManagerError::UnsafeName { .. }));
    assert!(!fetched.get(), "fetch must not run for an unsafe name");
    assert!(!tmp.path().join("evil.bin").exists());

    // A traversal `id` is rejected too.
    let evil_id = ModelSpec {
        kind: ModelKind::Asr,
        id: "../escape".into(),
        files: vec![ModelFile {
            name: "m.bin".into(),
            url: "u".into(),
            sha256: String::new(),
        }],
    };
    assert!(matches!(
        mgr.ensure(&evil_id, |_| Ok(vec![])).unwrap_err(),
        ManagerError::UnsafeName { .. }
    ));
}

#[test]
fn empty_digest_skips_verification() {
    let tmp = tempfile::tempdir().unwrap();
    let mgr = ModelManager::new(tmp.path());
    let s = spec(vec![ModelFile {
        name: "m.bin".into(),
        url: "u".into(),
        sha256: String::new(),
    }]);
    mgr.ensure(&s, |_| Ok(b"anything".to_vec())).unwrap();
    assert!(mgr.is_present(&s));
}
