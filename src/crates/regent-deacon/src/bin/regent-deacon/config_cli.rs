//! `regent-deacon config <op>` — the offline config surface (plan Phase B).
//!
//! The deacon is normally a stdio JSON-RPC daemon. These subcommands are the
//! same validated operations reachable as a short-lived process: no store, no
//! provider, no sockets, no daemon. That is what lets the CLI keep exactly ONE
//! implementation of "change a config key" while still working when config.yaml
//! is bad enough that the daemon cannot start.
//!
//! Output is JSON on stdout, diagnostics on stderr, and the exit codes follow
//! the CLI's taxonomy: 0 ok, 1 failed, 2 bad invocation.

use regent_deacon::{
    Validation, describe_config, set_config_paths, unset_config_path, validate_config,
};
use serde_json::{Value, json};
use std::path::Path;

const USAGE: &str =
    "usage: regent-deacon config <describe|validate|set <key> <json-value>...|unset <key>>";

/// Runs an offline config op. `args` excludes argv[0] and the `config` verb.
pub fn run(home: &Path, args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("describe") => emit(&describe_config(home)),
        Some("validate") => match validate_config(home) {
            Validation::Ok => emit(&json!({ "status": "ok" })),
            // Kept apart on purpose: one is repairable with `unset`, the other
            // has to be fixed by hand, and the advice differs.
            Validation::Malformed(detail) => fail("malformed", &detail),
            Validation::Invalid(detail) => fail("invalid", &detail),
        },
        // `set <key> <value> [<key> <value> …]` — several keys in ONE
        // transaction, so a command that configures a group of related keys
        // cannot leave half of them applied.
        Some("set") => {
            let pairs = &args[1..];
            if pairs.is_empty() || !pairs.len().is_multiple_of(2) {
                return usage();
            }
            // Classify BEFORE attempting the write, so "not YAML at all" is
            // reported as `malformed` here exactly as `validate` reports it.
            // Collapsing it into `invalid` told the caller the value was
            // wrong when the file was the problem.
            if let Validation::Malformed(detail) = validate_config(home) {
                return fail("malformed", &detail);
            }
            // Values arrive as JSON so a list stays a list and a number stays a
            // number; a bare string is accepted for convenience.
            let edits: Vec<(String, Value)> = pairs
                .chunks_exact(2)
                .map(|kv| {
                    let raw = &kv[1];
                    let value =
                        serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.clone()));
                    (kv[0].clone(), value)
                })
                .collect();
            match set_config_paths(home, &edits) {
                Ok((changed, _)) => emit(&json!({ "status": "ok", "changed": changed })),
                Err(detail) => fail("invalid", &detail),
            }
        }
        Some("unset") => match args.get(1) {
            Some(key) => {
                // Same classification as `set`, for the same reason: reporting
                // an unparseable FILE as an invalid KEY sends the caller looking
                // at the key they typed, which is not the problem.
                if let Validation::Malformed(detail) = validate_config(home) {
                    return fail("malformed", &detail);
                }
                match unset_config_path(home, key) {
                    Ok(true) => emit(&json!({ "status": "ok", "removed": key })),
                    Ok(false) => fail("not_set", &format!("{key} is not set in config.yaml")),
                    Err(detail) => fail("invalid", &detail),
                }
            }
            None => usage(),
        },
        _ => usage(),
    }
}

fn emit(value: &Value) -> i32 {
    println!("{value}");
    0
}

fn fail(status: &str, detail: &str) -> i32 {
    println!("{}", json!({ "status": status, "detail": detail }));
    1
}

fn usage() -> i32 {
    eprintln!("{USAGE}");
    2
}
