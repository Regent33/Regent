//! `.env` primitives: BOM tolerance, the credential merge, the per-turn skip
//! that keeps it off the hot path, and the Windows SID parse. Everything here
//! goes through a pure helper or a tempdir — never the global `REGENT_HOME`,
//! which the other tests in this crate mutate in parallel.

use super::*;

#[test]
fn leading_bom_does_not_hide_the_first_env_var() {
    // A .env written with a UTF-8 BOM (editors/PowerShell) must still expose
    // its first key — regression for REGENT_API_KEY showing as "not set".
    // Tested at the read layer directly to avoid racing on the global
    // REGENT_HOME env var with the other tests.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".env");
    std::fs::write(
        &path,
        "\u{feff}REGENT_API_KEY=sk-or-abcd1234\nOLLAMA_API_KEY=ol-xyz9\n",
    )
    .unwrap();
    let lines = read_lines(&path);
    // The BOM sits only at the file start, so it can hide ONLY the first
    // key — assert both the first (was hidden) and a later one resolve.
    assert_eq!(
        line_index(&lines, "REGENT_API_KEY"),
        Some(0),
        "BOM must not hide the first var"
    );
    assert_eq!(
        line_index(&lines, "OLLAMA_API_KEY"),
        Some(1),
        "later vars unaffected"
    );
}

#[test]
fn reload_applies_changed_credentials_and_skips_the_rest() {
    // Unique var name → no interference with parallel tests; tested via the
    // pure helper so it never races the global REGENT_HOME.
    let var = "TEST_RELOAD_ONLY_API_KEY";
    unsafe { std::env::remove_var(var) };
    let lines = vec![
        format!("{var}=v1"),
        "TEST_RELOAD_ONLY_MODEL=gpt".to_owned(), // not a credential → skipped
        "# a comment".to_owned(),
    ];
    assert_eq!(apply_credential_lines(&lines), 1, "credential applied");
    assert_eq!(std::env::var(var).ok().as_deref(), Some("v1"));
    assert!(
        std::env::var("TEST_RELOAD_ONLY_MODEL").is_err(),
        "non-credential var must not be merged"
    );
    // Unchanged value → no churn.
    assert_eq!(
        apply_credential_lines(&lines),
        0,
        "no re-apply when unchanged"
    );
    // Changed value → applied.
    assert_eq!(apply_credential_lines(&[format!("{var}=v2")]), 1);
    assert_eq!(std::env::var(var).ok().as_deref(), Some("v2"));
    unsafe { std::env::remove_var(var) };
}

#[test]
fn an_unmoved_dotenv_is_skipped_but_an_edited_one_is_not() {
    // The turn-path skip: first sight reads, a second look at the same bytes
    // does not, and a real edit is picked up. A key saved mid-session still
    // has to apply THIS turn — that was a shipped bug fix, so the "changed"
    // half of this matters more than the "unchanged" half.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".env");
    let mut last = None;
    assert!(
        !dotenv_moved(&path, &mut last),
        "a .env that does not exist has nothing to merge"
    );

    std::fs::write(&path, "A_KEY=v1\n").unwrap();
    assert!(dotenv_moved(&path, &mut last), "first sight always reads");
    assert!(!dotenv_moved(&path, &mut last), "unchanged file is skipped");

    // A longer value moves the length even if the clock has not ticked, which
    // is the case a same-tick mtime alone would miss.
    std::fs::write(&path, "A_KEY=v2-longer\n").unwrap();
    assert!(dotenv_moved(&path, &mut last), "an edit must be picked up");
    assert!(!dotenv_moved(&path, &mut last), "and only picked up once");
}

#[cfg(windows)]
#[test]
fn parses_the_process_token_sid_from_whoami_csv() {
    assert_eq!(
        parse_whoami_sid("\"machine\\user\",\"S-1-5-21-1-2-3-1001\""),
        Some("S-1-5-21-1-2-3-1001".into())
    );
}
