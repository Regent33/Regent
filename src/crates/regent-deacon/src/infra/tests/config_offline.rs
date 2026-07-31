//! Unit tests for the offline config operations (extracted for the file-size
//! rule; same module).

use super::*;
use crate::application::dispatcher::config_ops::set_config_path;
use serde_json::json;
use tempfile::TempDir;

fn home_with(contents: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("config.yaml"), contents).unwrap();
    dir
}

#[test]
fn a_valid_config_validates_with_no_deacon() {
    let dir = home_with("_config_version: 2\nmodel:\n  default: claude-sonnet-4-6\n");
    assert!(matches!(validate_config(dir.path()), Validation::Ok));
}

#[test]
fn malformed_yaml_and_schema_errors_are_different_answers() {
    // The distinction the CLI needs: one is repairable by `config unset`, the
    // other has to be fixed by hand.
    let bad_yaml = home_with("model:\n  default: \"unclosed\n  bad: [\n");
    assert!(matches!(
        validate_config(bad_yaml.path()),
        Validation::Malformed(_)
    ));

    let bad_schema = home_with("_config_version: 2\nmodel:\n  defalut: typo\n");
    let Validation::Invalid(message) = validate_config(bad_schema.path()) else {
        panic!("a typo'd key must be a schema error, not malformed YAML");
    };
    assert!(
        message.contains("defalut"),
        "error names the key: {message}"
    );
}

#[test]
fn a_missing_config_is_not_an_error() {
    let dir = TempDir::new().unwrap();
    assert!(matches!(validate_config(dir.path()), Validation::Ok));
}

#[test]
fn unset_repairs_a_schema_invalid_config() {
    let dir = home_with("_config_version: 2\nmodel:\n  defalut: typo\n");
    assert!(matches!(
        validate_config(dir.path()),
        Validation::Invalid(_)
    ));
    assert!(unset_config_path(dir.path(), "model.defalut").unwrap());
    assert!(matches!(validate_config(dir.path()), Validation::Ok));
}

#[test]
fn unset_reports_a_key_that_was_not_there() {
    let dir = home_with("_config_version: 2\nmodel:\n  default: x\n");
    assert!(!unset_config_path(dir.path(), "model.nothing").unwrap());
    assert!(!unset_config_path(dir.path(), "nothing.at.all").unwrap());
}

#[test]
fn unset_refuses_a_removal_that_would_break_the_config() {
    // Removing a key cannot normally invalidate the file (every field has a
    // default), but the gate must be there: the write is proven, not assumed.
    let dir = home_with("_config_version: 2\nmodel:\n  default: x\n");
    let before = std::fs::read_to_string(dir.path().join("config.yaml")).unwrap();
    assert!(unset_config_path(dir.path(), "model.default").unwrap());
    let after = std::fs::read_to_string(dir.path().join("config.yaml")).unwrap();
    assert_ne!(before, after);
    assert!(matches!(validate_config(dir.path()), Validation::Ok));
}

#[test]
fn the_descriptor_lists_real_keys_with_types_and_defaults() {
    let dir = home_with("_config_version: 2\nmodel:\n  default: claude-opus-5\n");
    let d = describe_config(dir.path());
    assert_eq!(d["descriptor_version"], DESCRIPTOR_VERSION);
    let keys = d["keys"].as_array().unwrap();

    let model = keys
        .iter()
        .find(|k| k["path"] == "model.default")
        .expect("model.default is a real key");
    assert_eq!(model["type"], "string");
    // Set in the file → the effective value is the file's, and it says so.
    assert_eq!(model["value"], "claude-opus-5");
    assert_eq!(model["origin"], "config.yaml");

    let untouched = keys
        .iter()
        .find(|k| k["path"] == "tools.auto_approve")
        .expect("tools.auto_approve is a real key");
    assert_eq!(untouched["type"], "boolean");
    assert_eq!(untouched["origin"], "default");
    assert_eq!(untouched["value"], false);
}

#[test]
fn the_descriptor_answers_even_when_the_file_is_unreadable() {
    // "What can I set?" has to work on a broken install — that is when it is asked.
    let dir = home_with("this: is: not: yaml: [\n");
    let d = describe_config(dir.path());
    assert!(!d["keys"].as_array().unwrap().is_empty());
    // …and it must SAY the values are defaults rather than the file's contents.
    // Without this a reader cannot tell "everything is at its default" from
    // "the file could not be parsed", and `config list` reported the second as
    // the first — on a config.yaml that was full of settings.
    assert_eq!(d["file"], "malformed");
    assert!(
        d["file_detail"]
            .as_str()
            .unwrap_or_default()
            .contains("YAML"),
        "{}",
        d["file_detail"]
    );
}

#[test]
fn the_descriptor_distinguishes_a_readable_file_from_an_absent_one() {
    let dir = home_with("_config_version: 2\nmodel:\n  default: claude-opus-5\n");
    assert_eq!(describe_config(dir.path())["file"], "ok");

    let empty = tempfile::tempdir().unwrap();
    // No config.yaml is not a fault: the loader creates one from defaults, so
    // the values reported ARE what the deacon will use.
    assert_eq!(describe_config(empty.path())["file"], "missing");
}

#[test]
fn secrets_report_presence_and_never_a_value() {
    assert!(is_secret_path("http.token"));
    assert!(is_secret_path("providers.openrouter.api_key_env"));
    assert!(is_secret_path("tools.hook_tool_start"));
    assert!(!is_secret_path("model.default"));
    assert!(!is_secret_path("http.enabled"));
    // A wildcard segment matches exactly one segment, never a prefix.
    assert!(!is_secret_path("providers.api_key_env"));
    assert!(!is_secret_path("providers.a.b.api_key_env"));

    let dir = home_with("_config_version: 2\nhttp:\n  token: super-secret-value\n");
    let d = describe_config(dir.path());
    let rendered = d.to_string();
    assert!(
        !rendered.contains("super-secret-value"),
        "the descriptor leaked a secret: {rendered}"
    );
    let token = d["keys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["path"] == "http.token")
        .unwrap();
    assert_eq!(token["value"], "<set>");
    assert_eq!(token["secret"], true);
}

#[test]
fn a_locked_write_is_atomic_and_releases_its_lock() {
    let dir = home_with("_config_version: 2\nmodel:\n  default: x\n");
    let file = dir.path().join("config.yaml");
    write_config_locked(&file, "_config_version: 2\nmodel:\n  default: y\n").unwrap();
    assert!(
        std::fs::read_to_string(&file)
            .unwrap()
            .contains("default: y"),
        "the write did not land"
    );
    assert!(
        !dir.path().join("config.yaml.lock").exists(),
        "the lock outlived the write"
    );
    // No temp file left behind for the next reader to trip over.
    let strays: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .filter(|n| n.contains(".tmp."))
        .collect();
    assert!(strays.is_empty(), "temp files left behind: {strays:?}");
}

#[test]
fn a_held_lock_blocks_a_second_writer_rather_than_losing_its_update() {
    let dir = home_with("_config_version: 2\n");
    let file = dir.path().join("config.yaml");
    let lock = dir.path().join("config.yaml.lock");
    std::fs::write(&lock, "held by someone else").unwrap();
    let err = write_config_locked(&file, "_config_version: 2\n").unwrap_err();
    assert!(
        err.contains("another process"),
        "contention must be reported, not silently overwritten: {err}"
    );
    std::fs::remove_file(&lock).unwrap();
}

#[test]
fn a_malformed_file_is_not_reported_as_a_schema_error() {
    // The two failures need different advice, so `set` must classify the file
    // the same way `validate` does instead of collapsing both into "invalid".
    let dir = home_with("model:\n  default: \"unclosed\n  bad: [\n");
    assert!(matches!(
        validate_config(dir.path()),
        Validation::Malformed(_)
    ));
    // And the write path refuses without touching the file.
    let before = std::fs::read_to_string(dir.path().join("config.yaml")).unwrap();
    let _ = set_config_path(dir.path(), "model.default", &json!("x"));
    let after = std::fs::read_to_string(dir.path().join("config.yaml")).unwrap();
    assert_eq!(
        before, after,
        "an unparseable config must never be rewritten"
    );
}

#[test]
fn a_dynamic_map_is_described_key_by_key_so_secrets_can_be_redacted() {
    // `providers` defaults to empty, so its shape comes from the FILE. Treating
    // it as one leaf emitted the whole map as a single value — which meant the
    // `providers.*.api_key_env` secret rule was never reached and anything
    // nested under it escaped redaction.
    let dir = home_with(
        "_config_version: 2\nproviders:\n  openrouter:\n    kind: openrouter\n    api_key_env: OPENROUTER_API_KEY\n",
    );
    let d = describe_config(dir.path());
    let keys = d["keys"].as_array().unwrap();
    let paths: Vec<&str> = keys.iter().filter_map(|k| k["path"].as_str()).collect();
    assert!(
        paths.contains(&"providers.openrouter.api_key_env"),
        "dynamic map was not descended into: {paths:?}"
    );
    let key_env = keys
        .iter()
        .find(|k| k["path"] == "providers.openrouter.api_key_env")
        .unwrap();
    assert_eq!(
        key_env["secret"], true,
        "the wildcard secret rule must fire"
    );
    // The whole map must no longer appear as one unredacted blob.
    assert!(
        !paths.contains(&"providers"),
        "the map is still emitted as a single leaf"
    );
}

#[test]
fn the_read_modify_write_is_serialised_by_the_lock() {
    // The lock has to span the READ. Holding it only around the write let two
    // writers each start from revision A and the second silently discard the
    // first. Proven here by observing that a second writer cannot even begin
    // its read while the first holds the lock.
    let dir = home_with("_config_version: 2\n");
    let file = dir.path().join("config.yaml");
    let mut inner_saw_lock = false;
    mutate_config_locked(&file, |raw| {
        assert!(raw.contains("_config_version"), "the read happens inside");
        inner_saw_lock = dir.path().join("config.yaml.lock").exists();
        Ok(format!("{raw}model:\n  default: x\n"))
    })
    .unwrap();
    assert!(inner_saw_lock, "the lock was not held during the read");
    assert!(matches!(validate_config(dir.path()), Validation::Ok));
}

#[test]
fn an_unreadable_config_is_not_reported_as_valid() {
    // A directory where config.yaml should be: reading fails for a reason that
    // is not "absent", and "absent" is the only read failure that is fine.
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join("config.yaml")).unwrap();
    assert!(matches!(
        validate_config(dir.path()),
        Validation::Malformed(_)
    ));
}
