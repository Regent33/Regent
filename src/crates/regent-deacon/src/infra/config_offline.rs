//! Config operations that answer WITHOUT a running deacon (plan Phase B).
//!
//! Why this exists: the CLI had its own YAML writer, so there were two
//! implementations of "change a config key" and only the Rust one validated.
//! The obvious fix — always use the RPC — fails exactly when it matters, because
//! a config bad enough to fail validation is a config the deacon will not start
//! with, and then the RPC is gone too.
//!
//! So the validated implementation moves down here, where a short-lived process
//! can run it with no daemon, no store, no provider and no sockets. The RPC path
//! and the `regent-deacon config …` subcommand call the same code.
//!
//! ponytail: the descriptor is derived by serialising `DeaconConfig::default()`,
//! which gives every key path, its type and its default straight from the Rust
//! type — no source scraping, no second schema to drift. It does NOT carry the
//! doc comments; a `schemars` derive is the upgrade path when descriptions are
//! worth ~20 derives across the config modules.

use crate::domain::config::{CURRENT_CONFIG_VERSION, DeaconConfig};
use serde_json::{Value, json};
use std::path::Path;

/// Replace config.yaml with `contents`, holding an exclusive lock for the whole
/// read-modify-write and swapping the file in atomically.
///
/// Every writer goes through here — the RPC, the offline subcommand, the desktop
/// app. Two processes doing read → edit → write can otherwise both read the same
/// revision and the second write silently discards the first, which no amount of
/// validation catches. The temp file is flushed before the rename so a crash
/// cannot leave a truncated config in place of a working one.
///
/// ponytail: a crashed writer leaves the lock file behind and the error names it
/// for deletion. Stealing it on an age heuristic is the upgrade path if that ever
/// actually bites someone.
pub fn write_config_locked(file: &Path, contents: &str) -> Result<(), String> {
    mutate_config_locked(file, |_| Ok(contents.to_owned()))
}

/// Run a whole read-modify-write under the lock.
///
/// `edit` receives the CURRENT file contents (empty when absent) and returns the
/// replacement. Holding the lock only around the write was not enough: two
/// processes could each read revision A, each produce their own B and C, and
/// write them in turn — the second silently discarding the first. Validation
/// stops a BAD write; only this stops a LOST one, and it has to span the read.
pub fn mutate_config_locked<F>(file: &Path, edit: F) -> Result<(), String>
where
    F: FnOnce(&str) -> Result<String, String>,
{
    use std::io::Write;
    let lock_path = file.with_extension("yaml.lock");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let lock = loop {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(f) => break f,
            // Only "already exists" is contention; a permission or path error is
            // a real failure and must not be retried for five seconds.
            Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => {
                return Err(format!("cannot lock config.yaml: {e}"));
            }
            Err(_) if std::time::Instant::now() >= deadline => {
                return Err(format!(
                    "another process is writing config.yaml. If none is running, delete {}",
                    lock_path.display()
                ));
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    };
    let result = (|| {
        // Read INSIDE the lock — that is the whole point of this function.
        let current = match std::fs::read_to_string(file) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(format!("cannot read config.yaml: {e}")),
        };
        let contents = edit(&current)?;
        let tmp = file.with_extension(format!("yaml.tmp.{}", std::process::id()));
        let write = (|| -> Result<(), String> {
            let mut f =
                std::fs::File::create(&tmp).map_err(|e| format!("cannot write config: {e}"))?;
            f.write_all(contents.as_bytes())
                .map_err(|e| format!("cannot write config: {e}"))?;
            f.sync_all()
                .map_err(|e| format!("cannot flush config: {e}"))?;
            drop(f);
            // `fs::rename` replaces an existing destination on Windows too
            // (MoveFileEx + MOVEFILE_REPLACE_EXISTING) — asserted by the tests.
            std::fs::rename(&tmp, file).map_err(|e| format!("cannot replace config.yaml: {e}"))
        })();
        // Never leave a temp file behind for the next reader to trip over.
        if write.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        write
    })();
    drop(lock);
    let _ = std::fs::remove_file(&lock_path);
    result
}

/// Bumped when the SHAPE of the descriptor changes — not when config keys do.
/// A CLI reading a descriptor from a newer deacon checks this before trusting
/// the field names, which is the CLI/deacon skew case.
pub const DESCRIPTOR_VERSION: u32 = 1;

/// Key paths whose value must never be printed. Presence is reportable; the
/// value is not. Matching is on the dotted path, with `*` for a map key.
const SECRET_PATHS: &[&str] = &[
    "http.token",
    "tools.hook_tool_start",
    "tools.hook_tool_complete",
    "providers.*.api_key_env",
];

/// True when a dotted path names a secret. `*` matches exactly one segment.
#[must_use]
pub fn is_secret_path(path: &str) -> bool {
    SECRET_PATHS.iter().any(|pattern| {
        let (p, k): (Vec<&str>, Vec<&str>) =
            (pattern.split('.').collect(), path.split('.').collect());
        p.len() == k.len() && p.iter().zip(&k).all(|(a, b)| *a == "*" || a == b)
    })
}

/// One key of the config, as the Rust type defines it.
fn describe_into(out: &mut Vec<Value>, prefix: &str, default: &Value, current: Option<&Value>) {
    if let Value::Object(map) = default {
        for (k, v) in map {
            let path = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{prefix}.{k}")
            };
            let cur = current.and_then(|c| c.get(k));
            // Recurse into sections. A DYNAMIC map (`providers`, `mom`) defaults
            // to empty, so its shape comes from the file rather than the type —
            // descend into what is actually there. Treating it as a leaf emitted
            // the whole map as one value, which meant `providers.*.api_key_env`
            // never matched and nothing under it could be redacted.
            if v.is_object() {
                let shape = if v.as_object().is_some_and(serde_json::Map::is_empty) {
                    cur.filter(|c| c.is_object())
                } else {
                    Some(v)
                };
                match shape {
                    Some(shape) => describe_into(out, &path, shape, cur),
                    // Empty in the type AND empty in the file: report the section
                    // itself so `config list --all` still names it.
                    None => out.push(leaf(&path, v, cur)),
                }
            } else {
                out.push(leaf(&path, v, cur));
            }
        }
    }
}

fn leaf(path: &str, default: &Value, current: Option<&Value>) -> Value {
    let secret = is_secret_path(path);
    let effective = current.unwrap_or(default);
    let set_in_file = current.is_some_and(|c| c != default);
    json!({
        "path": path,
        "type": type_name(default),
        "default": if secret { json!("<redacted>") } else { default.clone() },
        "value": if secret {
            json!(if is_present(effective) { "<set>" } else { "<unset>" })
        } else {
            effective.clone()
        },
        // Where the value came from. The deacon reads config.yaml and env
        // separately per key, so this reports the file-vs-default distinction
        // only; env overrides are reported by `doctor`/`security`.
        "origin": if set_in_file { "config.yaml" } else { "default" },
        "secret": secret,
    })
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) => {
            if n.is_f64() {
                "number"
            } else {
                "integer"
            }
        }
        Value::String(_) => "string",
        Value::Array(_) => "list",
        Value::Object(_) => "map",
    }
}

fn is_present(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
        _ => true,
    }
}

/// The full descriptor: every key the Rust config type defines, with its type,
/// default, effective value and origin. Secrets report presence only.
///
/// `home` may hold no config.yaml, or one that does not parse — either way the
/// descriptor still lists every key, because "what can I even set?" is a
/// question a broken install has to be able to answer.
#[must_use]
pub fn describe_config(home: &Path) -> Value {
    let defaults = serde_json::to_value(DeaconConfig::default()).unwrap_or_else(|_| json!({}));
    let current = std::fs::read_to_string(home.join("config.yaml"))
        .ok()
        .and_then(|raw| serde_yaml::from_str::<Value>(&raw).ok());
    let mut keys = Vec::new();
    describe_into(&mut keys, "", &defaults, current.as_ref());
    json!({
        "descriptor_version": DESCRIPTOR_VERSION,
        "config_version": CURRENT_CONFIG_VERSION,
        "keys": keys,
    })
}

/// Outcome of an offline validation, kept apart because the two failures need
/// different advice: unparseable YAML has to be fixed by hand, a schema error
/// can be repaired by removing the offending key.
pub enum Validation {
    Ok,
    /// The file is not YAML at all.
    Malformed(String),
    /// The file is YAML but not a config: unknown key, bad enum, wrong type.
    Invalid(String),
}

/// Validate `$home/config.yaml` against the real config type, with no deacon.
#[must_use]
pub fn validate_config(home: &Path) -> Validation {
    let path = home.join("config.yaml");
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        // Absent → the loader creates it from defaults, which is fine. Any OTHER
        // read failure (permissions, a directory, a sharing violation) is NOT a
        // valid config and must never be reported as one.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Validation::Ok,
        Err(e) => return Validation::Malformed(format!("cannot read config.yaml: {e}")),
    };
    if let Err(e) = serde_yaml::from_str::<serde_yaml::Value>(&raw) {
        return Validation::Malformed(e.to_string());
    }
    match serde_yaml::from_str::<DeaconConfig>(&raw) {
        Ok(cfg) => match cfg.agents_defaults.validate() {
            Ok(()) => Validation::Ok,
            Err(e) => Validation::Invalid(e),
        },
        Err(e) => Validation::Invalid(e.to_string()),
    }
}

/// Remove a dotted key and prove the result still deserialises, exactly like
/// `set_config_path` does for a write. Repairing a config must not be able to
/// break it further.
pub fn unset_config_path(home: &Path, path: &str) -> Result<bool, String> {
    let file = home.join("config.yaml");
    let segments: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return Err("empty path".to_owned());
    }
    // "the key was not there" is not an error, but it also must not write. The
    // flag carries that decision out of the closure.
    let mut removed = false;
    mutate_config_locked(&file, |raw| {
        let mut doc: serde_yaml::Value =
            serde_yaml::from_str(raw).map_err(|e| format!("config.yaml is not valid YAML: {e}"))?;
        let mut cur = &mut doc;
        for seg in &segments[..segments.len() - 1] {
            let Some(next) = cur
                .as_mapping_mut()
                .and_then(|m| m.get_mut(serde_yaml::Value::from(*seg)))
            else {
                return Ok(raw.to_owned()); // nothing to remove — rewrite as-is
            };
            cur = next;
        }
        let last = *segments.last().unwrap();
        let Some(map) = cur.as_mapping_mut() else {
            return Ok(raw.to_owned());
        };
        if map.remove(serde_yaml::Value::from(last)).is_none() {
            return Ok(raw.to_owned());
        }
        let out = serde_yaml::to_string(&doc).map_err(|e| e.to_string())?;
        serde_yaml::from_str::<DeaconConfig>(&out)
            .map_err(|e| format!("rejected — removing '{path}' would break config.yaml: {e}"))?;
        removed = true;
        Ok(out)
    })?;
    Ok(removed)
}

#[cfg(test)]
#[path = "tests/config_offline.rs"]
mod tests;
