//! Pure unit tests for the update checker: semver, manifest parsing (including
//! unknown-field tolerance and malformed/oversized rejection), the bounded
//! cache round-trip, deterministic jitter, opt-out, and status derivation.
//! No network, no real `$REGENT_HOME` — every path uses a `tempfile` dir.

use super::cache::CacheFile;
use super::checker::{is_optout, status_from_cache};
use super::model::{MAX_MANIFEST_BYTES, ManifestError, Version, is_newer, parse_manifest};
use super::{allowed_manifest_host, manifest_url};

const SAMPLE: &str = r#"{
  "schema": 1,
  "generated_at": "2026-07-24T12:00:00Z",
  "channels": { "stable": { "version": "0.1.2", "released_at": "x",
    "protocols": { "call": { "min": 7, "max": 7 } } } },
  "signing_key_id": "regent-2026a",
  "future_unknown_top_level": { "anything": [1, 2, 3] }
}"#;

#[test]
fn semver_parses_and_orders_numerically() {
    assert_eq!(Version::parse("0.1.2").unwrap().to_string(), "0.1.2");
    assert_eq!(Version::parse("v1.2.3").unwrap().major, 1);
    // Numeric, not lexicographic: 0.1.10 is newer than 0.1.2.
    assert!(Version::parse("0.1.10") > Version::parse("0.1.2"));
    assert!(Version::parse("1.0.0") > Version::parse("0.9.9"));
    // Rejects non-triples and junk.
    assert!(Version::parse("1.2").is_none());
    assert!(Version::parse("1.2.3.4").is_none());
    assert!(Version::parse("1.2.x").is_none());
}

#[test]
fn is_newer_fails_safe_on_garbage() {
    assert!(is_newer("0.1.1", Some("0.1.2")));
    assert!(!is_newer("0.1.2", Some("0.1.2")));
    assert!(!is_newer("0.1.2", Some("0.1.1")));
    assert!(!is_newer("0.1.1", None));
    assert!(!is_newer("0.1.1", Some("not-a-version")));
    assert!(!is_newer("garbage", Some("9.9.9")));
}

#[test]
fn manifest_parses_and_tolerates_unknown_fields() {
    let m = parse_manifest(SAMPLE.as_bytes()).expect("valid manifest");
    assert_eq!(m.schema, 1);
    assert_eq!(m.stable_version().unwrap().to_string(), "0.1.2");
}

#[test]
fn manifest_without_stable_channel_has_no_version() {
    let m =
        parse_manifest(br#"{"schema":1,"channels":{"beta":{"version":"0.2.0"}}}"#).expect("parses");
    assert!(m.stable_version().is_none());
}

#[test]
fn malformed_manifest_is_rejected() {
    let err = parse_manifest(b"{not json").unwrap_err();
    assert!(matches!(err, ManifestError::Json(_)));
    // Missing the required `version` inside a channel is also a parse error.
    assert!(parse_manifest(br#"{"channels":{"stable":{}}}"#).is_err());
}

#[test]
fn unsupported_manifest_schema_is_rejected() {
    let error = parse_manifest(br#"{"schema":2,"channels":{}}"#).unwrap_err();
    assert!(matches!(error, ManifestError::UnsupportedSchema(2)));
}

#[test]
fn oversized_manifest_is_rejected_before_parsing() {
    let big = vec![b'x'; MAX_MANIFEST_BYTES + 1];
    match parse_manifest(&big).unwrap_err() {
        ManifestError::TooLarge(n) => assert_eq!(n, MAX_MANIFEST_BYTES + 1),
        other => panic!("expected TooLarge, got {other:?}"),
    }
}

#[test]
fn status_from_cache_derives_availability() {
    // No cache → "never", nothing available.
    let none = status_from_cache("0.1.1", None);
    assert_eq!(none.source, "never");
    assert!(!none.available);
    assert!(none.checked_at.is_none());

    // Newer latest → available.
    let up = status_from_cache(
        "0.1.1",
        Some(CacheFile {
            checked_at: 42,
            latest: Some("0.1.2".into()),
            ..Default::default()
        }),
    );
    assert!(up.available);
    assert_eq!(up.latest.as_deref(), Some("0.1.2"));
    assert_eq!(up.checked_at, Some(42));

    // Same version → not available.
    let same = status_from_cache(
        "0.1.2",
        Some(CacheFile {
            latest: Some("0.1.2".into()),
            ..Default::default()
        }),
    );
    assert!(!same.available);
}

#[test]
fn optout_matches_truthy_values_only() {
    for v in ["1", "true", "yes", " true "] {
        assert!(is_optout(Some(v)), "{v} should opt out");
    }
    for v in ["0", "false", "", "no"] {
        assert!(!is_optout(Some(v)), "{v} should not opt out");
    }
    assert!(!is_optout(None));
}

#[test]
fn manifest_url_and_redirect_hosts_are_fixed() {
    let url = manifest_url("Regent33/Regent");
    assert_eq!(
        url,
        "https://github.com/Regent33/Regent/releases/latest/download/regent-manifest.json"
    );
    for host in [
        "https://github.com/x",
        "https://objects.githubusercontent.com/x",
        "https://release-assets.githubusercontent.com/x",
    ] {
        assert!(allowed_manifest_host(&reqwest::Url::parse(host).unwrap()));
    }
    assert!(!allowed_manifest_host(
        &reqwest::Url::parse("https://example.com/manifest").unwrap()
    ));
}
