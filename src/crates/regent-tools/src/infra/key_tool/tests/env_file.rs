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
    let lines = read_lines(&path).expect("the fixture is readable");
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
        "TEST_NUMBERED_API_KEY_2=backup".to_owned(),
        "TEST_RELOAD_ONLY_MODEL=gpt".to_owned(), // not a credential → skipped
        "TEST_NUL_API_KEY=bad\0value".to_owned(), // set_var would panic → skipped
        "# a comment".to_owned(),
    ];
    assert_eq!(apply_credential_lines(&lines), 2, "credentials applied");
    assert_eq!(std::env::var(var).ok().as_deref(), Some("v1"));
    assert_eq!(
        std::env::var("TEST_NUMBERED_API_KEY_2").ok().as_deref(),
        Some("backup")
    );
    assert!(
        std::env::var("TEST_RELOAD_ONLY_MODEL").is_err(),
        "non-credential var must not be merged"
    );
    assert!(
        std::env::var("TEST_NUL_API_KEY").is_err(),
        "a NUL value must never reach set_var"
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
    unsafe { std::env::remove_var("TEST_NUMBERED_API_KEY_2") };
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

#[test]
fn invalid_names_and_multiline_or_nul_values_never_reach_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".env");
    std::fs::write(&path, "SAFE_API_KEY=old\n").unwrap();
    let before = std::fs::read_to_string(&path).unwrap();

    for (key, value) in [
        ("REGENT_AUTO_APPROVE=1", "x"),
        ("SAFE_API_KEY", "ok\nREGENT_AUTO_APPROVE=1"),
        ("SAFE_API_KEY", "nul\0value"),
    ] {
        assert!(
            validate_env_pair(key, value).is_err(),
            "{key:?}/{value:?} must be refused"
        );
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
}

#[test]
fn a_protection_failure_keeps_the_live_file_and_cleans_the_temporary() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".env");
    std::fs::write(&path, "SAFE_API_KEY=old\n").unwrap();

    let error = write_lines_with(&path, &["SAFE_API_KEY=new".to_owned()], |_| {
        Err("ACL denied".to_owned())
    })
    .unwrap_err();
    assert!(error.contains("ACL denied"));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "SAFE_API_KEY=old\n"
    );
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp."))
        .collect();
    assert!(leftovers.is_empty(), "temporary secret file leaked");
}

#[test]
fn an_existing_file_is_replaced_as_one_complete_payload() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".env");
    std::fs::write(&path, "SAFE_API_KEY=old\nOTHER=value\n").unwrap();

    write_lines(&path, &["SAFE_API_KEY=new".to_owned()]).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "SAFE_API_KEY=new\n"
    );
}

#[test]
fn setting_normalizes_duplicates_and_removal_deletes_every_assignment() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".env");
    std::fs::write(
        &path,
        "TAVILY_API_KEY=old-one\nOTHER=value\n TAVILY_API_KEY=old-two\n",
    )
    .unwrap();

    with_env_lock(&path, || {
        upsert_env_var_at(&path, "TAVILY_API_KEY", "canonical")
    })
    .unwrap();
    let after_set = std::fs::read_to_string(&path).unwrap();
    assert_eq!(after_set.matches("TAVILY_API_KEY=").count(), 1);
    assert!(after_set.contains("TAVILY_API_KEY=canonical"));

    with_env_lock(&path, || remove_env_var_at(&path, "TAVILY_API_KEY")).unwrap();
    let after_remove = std::fs::read_to_string(&path).unwrap();
    assert!(!after_remove.contains("TAVILY_API_KEY="));
    assert!(after_remove.contains("OTHER=value"));
}

#[test]
fn concurrent_writers_keep_every_credential() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".env");
    let mut writers = Vec::new();
    for slot in 0..5 {
        let path = path.clone();
        writers.push(std::thread::spawn(move || {
            let key = format!("CONCURRENT_{slot}_API_KEY");
            with_env_lock(&path, || upsert_env_var_at(&path, &key, "saved"))
        }));
    }
    for writer in writers {
        writer.join().unwrap().unwrap();
    }

    let saved = std::fs::read_to_string(&path).unwrap();
    for slot in 0..5 {
        assert!(
            saved.contains(&format!("CONCURRENT_{slot}_API_KEY=saved")),
            "writer {slot} was lost:\n{saved}"
        );
    }
    assert!(!path.with_file_name(".env.lock").exists());
}

#[test]
fn masking_a_unicode_key_tail_never_slices_inside_a_character() {
    assert_eq!(mask("secret-密钥-四五六七"), "****四五六七");
    assert_eq!(mask("密钥"), "****");
}

#[cfg(windows)]
#[test]
fn parses_the_process_token_sid_from_whoami_csv() {
    assert_eq!(
        parse_whoami_sid("\"machine\\user\",\"S-1-5-21-1-2-3-1001\""),
        Some("S-1-5-21-1-2-3-1001".into())
    );
}

/// A missing `.env` is empty; an UNREADABLE one is an error, never empty.
///
/// The distinction is the whole safety property of the write path, which is
/// read-modify-publish: `upsert_env_var_at` reads, drops the key's old line,
/// pushes the new one, and atomically replaces the file. When the read
/// swallowed its error and returned an empty `Vec`, that sequence replaced
/// every credential the user had with the single key being saved — silently,
/// atomically, and reporting success. A fresh install must still work, so
/// NotFound alone maps to empty.
#[test]
fn an_unreadable_env_is_an_error_while_a_missing_one_is_empty() {
    let dir = tempfile::tempdir().unwrap();

    let missing = dir.path().join(".env");
    assert_eq!(
        read_lines(&missing).expect("a fresh install has no .env yet"),
        Vec::<String>::new(),
        "a missing file is genuinely empty - the first save must succeed"
    );

    // A directory in the file's place: read_to_string fails with something
    // that is NOT NotFound, which is the class that used to be swallowed.
    let unreadable = dir.path().join("blocked");
    std::fs::create_dir(&unreadable).unwrap();
    let err = read_lines(&unreadable).expect_err("an unreadable .env must not read as empty");
    assert!(err.contains("cannot read"), "unexpected error: {err}");

    // And the write path refuses rather than publishing a one-line file.
    let refused = upsert_env_var_at(&unreadable, "ANTHROPIC_API_KEY", "sk-new")
        .expect_err("a write must not proceed on an unreadable file");
    assert!(
        refused.contains("cannot read"),
        "unexpected error: {refused}"
    );
}

/// The writer must recognise every assignment the loader honours.
///
/// `apply_credential_lines` trims the name after splitting on `=`, so
/// `KEY =value` is live. While `is_key_line` demanded that `=` touch the key,
/// `remove_env_var` skipped that line and the "deleted" credential loaded again
/// on the next boot; `upsert` left it as a second assignment. Both directions
/// are pinned here because the delete case is the one with teeth.
#[test]
fn a_spaced_assignment_is_the_same_key_to_writer_and_loader() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".env");
    std::fs::write(&path, "ANTHROPIC_API_KEY =sk-old\nOPENAI_API_KEY=sk-keep\n").unwrap();

    // The loader honours it, which is what makes the writer's blindness unsafe.
    let lines = read_lines(&path).unwrap();
    assert!(
        is_key_line(&lines[0], "ANTHROPIC_API_KEY"),
        "the writer must see the line the loader acts on: {:?}",
        lines[0]
    );

    // Removal really removes it.
    assert!(remove_env_var_at(&path, "ANTHROPIC_API_KEY").unwrap());
    let after = read_lines(&path).unwrap();
    assert!(
        !after.iter().any(|l| l.contains("sk-old")),
        "the deleted credential survived: {after:?}"
    );
    assert!(
        after.iter().any(|l| l.contains("sk-keep")),
        "removal took an unrelated key with it: {after:?}"
    );

    // And a replace leaves exactly one assignment, not two.
    std::fs::write(&path, "ANTHROPIC_API_KEY =sk-old\n").unwrap();
    upsert_env_var_at(&path, "ANTHROPIC_API_KEY", "sk-new").unwrap();
    let replaced = read_lines(&path).unwrap();
    assert_eq!(
        replaced
            .iter()
            .filter(|l| is_key_line(l, "ANTHROPIC_API_KEY"))
            .count(),
        1,
        "duplicate assignment left behind: {replaced:?}"
    );
    assert!(
        !replaced.iter().any(|l| l.contains("sk-old")),
        "{replaced:?}"
    );
}
