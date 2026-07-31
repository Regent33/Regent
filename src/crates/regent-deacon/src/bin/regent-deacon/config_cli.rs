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
    Validation, describe_config, set_config_path, unset_config_path, validate_config,
};
use serde_json::{Value, json};
use std::path::Path;

const USAGE: &str =
    "usage: regent-deacon config <describe|validate|set <key> <json-value>|unset <key>>";

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
        Some("set") => match (args.get(1), args.get(2)) {
            (Some(key), Some(raw)) => {
                // Classify BEFORE attempting the write, so "not YAML at all" is
                // reported as `malformed` here exactly as `validate` reports it.
                // Collapsing it into `invalid` told the caller the value was
                // wrong when the file was the problem.
                if let Validation::Malformed(detail) = validate_config(home) {
                    return fail("malformed", &detail);
                }
                // The value arrives as JSON so a list stays a list and a number
                // stays a number; a bare string is accepted for convenience.
                let value: Value =
                    serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.clone()));
                match set_config_path(home, key, &value) {
                    Ok((changed, _)) => emit(&json!({ "status": "ok", "changed": changed })),
                    Err(detail) => fail("invalid", &detail),
                }
            }
            _ => usage(),
        },
        Some("unset") => match args.get(1) {
            Some(key) => match unset_config_path(home, key) {
                Ok(true) => emit(&json!({ "status": "ok", "removed": key })),
                Ok(false) => fail("not_set", &format!("{key} is not set in config.yaml")),
                Err(detail) => fail("invalid", &detail),
            },
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
