//! What the redactor must mask and must leave alone. Split from
//! `redact.rs` (file-size rule).

use super::*;
use crate::redact_env::refresh_own_secrets;
use std::io::Write;

#[test]
fn redacting_writer_masks_before_the_inner_writer_sees_bytes() {
    let mut sink: Vec<u8> = Vec::new();
    {
        let mut writer = RedactingWriter::new(&mut sink);
        writeln!(writer, "auth failed for sk-ant-api03-LEAKED123456").unwrap();
    }
    let written = String::from_utf8(sink).unwrap();
    assert!(written.contains("sk-ant-api03-***"), "got: {written}");
    assert!(!written.contains("LEAKED123456"));
}

#[test]
fn masks_known_provider_key_prefixes_keeping_the_prefix() {
    let got = redact_secrets(r#"{"error":"bad key sk-ant-api03-AbCdEf123456 rejected"}"#);
    assert!(got.contains("sk-ant-api03-***"), "got: {got}");
    assert!(!got.contains("AbCdEf123456"));
}

#[test]
fn masks_openai_openrouter_slack_github_and_jwt() {
    for (raw, want) in [
        ("key=sk-AbCdEfGhIjKl", "sk-***"),
        ("key=sk-or-v1-AbCdEfGhIj", "sk-or-v1-***"),
        ("tok xoxb-1234567890-abcdef", "xoxb-***"),
        ("ghp_AbCdEf1234567890", "ghp_***"),
        ("eyJhbGciOiJIUzI1NiJ9abcdef", "eyJ***"),
    ] {
        let got = redact_secrets(raw);
        assert!(got.contains(want), "for {raw:?} got {got:?}");
    }
}

#[test]
fn masks_the_token_after_bearer() {
    let got = redact_secrets("Authorization: Bearer abcDEF123456opaque");
    assert!(got.contains("Bearer ***"), "got: {got}");
    assert!(!got.contains("abcDEF123456opaque"));
}

#[test]
fn leaves_ordinary_text_untouched() {
    let text = "tool execution failed: file /tmp/notes-2026.md not found (status 404)";
    assert_eq!(redact_secrets(text), text);
}

#[test]
fn bearer_only_masks_the_immediate_next_token() {
    // A later unrelated word is not masked.
    let got = redact_secrets("Bearer sk-AbCdEfGhIj then continue normally");
    assert!(got.contains("then continue normally"), "got: {got}");
}

#[test]
fn does_not_mask_a_bare_prefix() {
    assert_eq!(redact_secrets("use the sk- prefix"), "use the sk- prefix");
}

/// The layer that needs no vendor list. The workspace reads 106 credential env
/// vars and the prefix list names a handful; whatever shape a key has, if this
/// process holds it, its literal value must not reach a log.
///
/// Serialized with the other env-mutating test: `set_var` is process-global.
#[test]
fn this_processs_own_credential_values_are_masked_whatever_shape_they_have() {
    let _guard = env_lock();
    // A shape no prefix in the list recognises — the real gap.
    unsafe { std::env::set_var("REDACT_FIXTURE_API_KEY", "totally-opaque-value-99") };
    refresh_own_secrets();

    let got = redact_secrets("provider rejected totally-opaque-value-99 with 401");
    assert_eq!(got, "provider rejected *** with 401");

    // Also caught inside a URL, where the tokenizer would never isolate it.
    let got = redact_secrets("GET https://api.example.com/v1?key=totally-opaque-value-99&x=1");
    assert!(!got.contains("totally-opaque-value-99"), "got: {got}");

    unsafe { std::env::remove_var("REDACT_FIXTURE_API_KEY") };
    refresh_own_secrets();
}

/// A credential var set to something short would punch holes through every log
/// line containing that word. An unreadable log is its own outage.
#[test]
fn short_credential_values_are_not_armed() {
    let _guard = env_lock();
    unsafe { std::env::set_var("REDACT_FIXTURE_SHORT_TOKEN", "local") };
    refresh_own_secrets();

    let text = "connecting to local database on local disk";
    assert_eq!(redact_secrets(text), text);

    unsafe { std::env::remove_var("REDACT_FIXTURE_SHORT_TOKEN") };
    refresh_own_secrets();
}

/// `set_var` is process-global and these tests run in parallel by default.
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// The gap the prefix list left open, as the four real shapes that walked
/// straight through it. Every one is a credential with no recognisable
/// prefix, and `x-api-key` is named in this module's own threat model.
#[test]
fn a_named_credential_field_is_masked_whatever_shape_its_value_has() {
    for (raw, leaked) in [
        (
            r#"{"x-api-key":"cpa-62f37946094046e09b7bcc6318109889"}"#,
            "cpa-62f37946094046e09b7bcc6318109889",
        ),
        (
            "Authorization: Basic dXNlcjpwYXNzd29yZA==",
            "dXNlcjpwYXNzd29yZA",
        ),
        ("password=hunter2swordfish", "hunter2swordfish"),
        (
            "x-goog-api-key: AIzaSyD-9tSrke72PouQMnMX",
            "AIzaSyD-9tSrke72PouQMnMX",
        ),
    ] {
        let got = redact_secrets(raw);
        assert!(!got.contains(leaked), "leaked from {raw:?}: {got}");
        assert!(got.contains("***"), "nothing masked in {raw:?}: {got}");
    }
}

/// The scheme is not the secret. Masking it would throw away the one word
/// that tells a reader which credential leaked.
#[test]
fn the_auth_scheme_survives_so_the_log_still_says_what_was_masked() {
    assert_eq!(
        redact_secrets("Authorization: Bearer sk-ant-api03-AbCdEf123456"),
        "Authorization: Bearer sk-ant-api03-***"
    );
    assert_eq!(
        redact_secrets("authorization: Basic dXNlcjpwYXNz"),
        "authorization: Basic ***"
    );
}

/// A field name only arms the next token once `:` or `=` confirms it is a
/// field. Without that, every log line discussing tokens or passwords would
/// come back with holes in it.
#[test]
fn credential_words_in_ordinary_prose_do_not_mask_the_next_word() {
    for text in [
        "the token expired after 30 minutes",
        "password reset email sent to the owner",
        "secret scanning found no matches",
        "auth failed with status 401",
    ] {
        assert_eq!(redact_secrets(text), text, "over-masked: {text}");
    }
}
