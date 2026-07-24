use super::UpdateChecker;
use super::cache::{CacheFile, MAX_DIAGNOSTIC_CHARS, bound};
use super::lock::CheckLock;

#[test]
fn cache_round_trips_bounds_and_replaces() {
    let home = tempfile::tempdir().unwrap();
    assert!(CacheFile::load(home.path()).is_none());
    CacheFile {
        etag: Some("\"abc\"".into()),
        checked_at: 1_700_000_000,
        latest: Some("0.1.2".into()),
        diagnostic: Some("x".repeat(MAX_DIAGNOSTIC_CHARS + 50)),
    }
    .store(home.path())
    .unwrap();

    let back = CacheFile::load(home.path()).expect("cache reloads");
    assert_eq!(back.etag.as_deref(), Some("\"abc\""));
    assert_eq!(back.latest.as_deref(), Some("0.1.2"));
    assert_eq!(
        back.diagnostic.unwrap().chars().count(),
        MAX_DIAGNOSTIC_CHARS
    );

    CacheFile {
        checked_at: 1_800_000_000,
        latest: Some("0.1.3".into()),
        ..Default::default()
    }
    .store(home.path())
    .unwrap();
    let replaced = CacheFile::load(home.path()).expect("second write replaces");
    assert_eq!(replaced.latest.as_deref(), Some("0.1.3"));
}

#[test]
fn bound_truncates_by_characters() {
    assert_eq!(bound("short").chars().count(), 5);
    assert_eq!(
        bound(&"y".repeat(MAX_DIAGNOSTIC_CHARS * 2)).chars().count(),
        MAX_DIAGNOSTIC_CHARS
    );
}

#[test]
fn jitter_is_deterministic_and_bounded() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let first = UpdateChecker::new(a.path().to_path_buf(), "0.1.1".into());
    let same = UpdateChecker::new(a.path().to_path_buf(), "0.1.1".into());
    let other = UpdateChecker::new(b.path().to_path_buf(), "0.1.1".into());
    assert_eq!(first.jitter(), same.jitter());
    assert!((0..super::JITTER_WINDOW_SECS).contains(&first.jitter()));
    assert!((0..super::JITTER_WINDOW_SECS).contains(&other.jitter()));
}

#[test]
fn check_lock_is_exclusive_and_released() {
    let home = tempfile::tempdir().unwrap();
    let first = CheckLock::acquire(home.path()).expect("first process owns lock");
    assert!(CheckLock::acquire(home.path()).is_none());
    drop(first);
    assert!(CheckLock::acquire(home.path()).is_some());
}
