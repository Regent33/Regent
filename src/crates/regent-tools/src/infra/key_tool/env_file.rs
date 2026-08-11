//! `$REGENT_HOME/.env` file primitives: owner-only writes, hot-apply to the
//! process env, and masked reads (the raw value is never returned).

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// Insert or replace `KEY=value` in `$REGENT_HOME/.env`, with the same
/// owner-only-permission write as key storage. For non-secret knobs too
/// (the deacon's `voice.set` uses it for REGENT_WHISPER_SIZE).
pub fn upsert_env_var(key: &str, value: &str) -> Result<(), String> {
    let path = env_path()?;
    let mut lines = read_lines(&path);
    match line_index(&lines, key) {
        Some(i) => lines[i] = format!("{key}={value}"),
        None => lines.push(format!("{key}={value}")),
    }
    write_lines(&path, &lines)?;
    // Hot-apply: EVERY writer (env.set, the agent's manage_keys, voice.set)
    // takes effect in the running process, not just after a restart.
    // SAFETY: same set_var pattern the boot .env merge uses.
    unsafe { std::env::set_var(key, value) };
    Ok(())
}

/// Swap the VALUES of two `.env` keys (both must exist) — the multi-key
/// "activate" primitive: the runtime always resolves the base slot first, so
/// swapping slot N into the base makes it the active key while keeping the
/// other stored. Hot-applies both to the process env.
pub fn swap_env_vars(a: &str, b: &str) -> Result<(), String> {
    let path = env_path()?;
    let mut lines = read_lines(&path);
    let (ia, ib) = match (line_index(&lines, a), line_index(&lines, b)) {
        (Some(ia), Some(ib)) => (ia, ib),
        _ => return Err(format!("both {a} and {b} must be set to swap")),
    };
    let value_of = |line: &str| {
        line.split_once('=')
            .map(|(_, v)| v)
            .unwrap_or("")
            .to_owned()
    };
    let (va, vb) = (value_of(&lines[ia]), value_of(&lines[ib]));
    lines[ia] = format!("{a}={vb}");
    lines[ib] = format!("{b}={va}");
    write_lines(&path, &lines)?;
    // SAFETY: mirrors upsert_env_var's hot-apply.
    unsafe {
        std::env::set_var(a, &vb);
        std::env::set_var(b, &va);
    }
    Ok(())
}

/// Remove `KEY=...` from `$REGENT_HOME/.env`. Returns whether a line existed.
pub fn remove_env_var(key: &str) -> Result<bool, String> {
    let path = env_path()?;
    let mut lines = read_lines(&path);
    match line_index(&lines, key) {
        Some(i) => {
            lines.remove(i);
            write_lines(&path, &lines)?;
            // SAFETY: mirrors upsert_env_var's hot-apply.
            unsafe { std::env::remove_var(key) };
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Re-merge credential vars from `$REGENT_HOME/.env` into the running process
/// env, so a key saved AFTER this (possibly long-lived — e.g. the voice
/// server's) process started takes effect on the next read WITHOUT a restart.
/// Called at turn start. Only credential-suffixed names are applied (never a
/// runtime knob like REGENT_MODEL), and only when the value actually changed.
/// Returns how many values were updated.
#[must_use]
pub fn reload_credentials_from_dotenv() -> usize {
    // Every turn pays for this, on the async executor thread, and on the
    // overwhelming majority of turns the file has not moved since the last
    // one — so stat it first and skip the read+parse+set_var sweep when it
    // hasn't. Behaviour when it HAS moved is unchanged.
    static LAST_SEEN: Mutex<Option<(SystemTime, u64)>> = Mutex::new(None);
    let changed = match env_path() {
        Ok(path) => {
            let mut last = LAST_SEEN.lock().unwrap_or_else(|e| e.into_inner());
            if dotenv_moved(&path, &mut last) {
                apply_credential_lines(&read_lines(&path))
            } else {
                0
            }
        }
        Err(_) => 0,
    };
    // Re-arm log redaction on the new values. Without this a key added
    // mid-session is live for provider calls but absent from the mask set, so
    // the one window where it is most likely to appear in an error is exactly
    // the window where it would not be masked.
    if changed > 0 {
        let armed = regent_kernel::refresh_own_secrets();
        tracing::debug!(changed, armed, "credentials re-merged; redaction re-armed");
    }
    changed
}

/// Has `.env` changed since `last` saw it? Records the new stamp and says yes;
/// says no when the stamp matches or the file cannot be stat'd (a missing file
/// has nothing to merge, which is what the read path did anyway).
///
/// mtime+len can in principle miss a same-length rewrite inside one filesystem
/// timestamp tick, but the only writer that fast is this process — and every
/// in-process writer (`upsert_env_var`, `swap_env_vars`, `remove_env_var`)
/// already hot-applies to the process env, so it never needed the re-read.
/// What this path exists for is an edit from OUTSIDE the process, which happens
/// on human timescales.
///
/// State comes in by `&mut` rather than off the static, so the logic is
/// testable without racing the process-wide stamp.
fn dotenv_moved(path: &Path, last: &mut Option<(SystemTime, u64)>) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    let stamp = (
        meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        meta.len(),
    );
    if *last == Some(stamp) {
        return false;
    }
    *last = Some(stamp);
    true
}

/// Pure core of [`reload_credentials_from_dotenv`] (env_path split out so it's
/// testable without racing the global `REGENT_HOME`).
fn apply_credential_lines(lines: &[String]) -> usize {
    const CRED_SUFFIXES: [&str; 3] = ["_KEY", "_TOKEN", "_SECRET"];
    let mut changed = 0;
    for line in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        // Credentials only, and a plain identifier — never let a malformed
        // `.env` line reach set_var (which panics on '=' or NUL in the key).
        if value.is_empty()
            || !key
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            || key.is_empty()
            || !CRED_SUFFIXES.iter().any(|s| key.ends_with(s))
        {
            continue;
        }
        if std::env::var(key).ok().as_deref() != Some(value) {
            // SAFETY: same hot-apply pattern as upsert_env_var; the key is
            // validated as a plain identifier above, so set_var cannot panic.
            unsafe { std::env::set_var(key, value) };
            changed += 1;
        }
    }
    changed
}

/// `(is_set, masked_value)` for `key` in `$REGENT_HOME/.env` — the value itself
/// is NEVER returned, only a `****last4` mask, so a UI can show presence without
/// re-leaking the secret.
#[must_use]
pub fn env_var_status(key: &str) -> (bool, Option<String>) {
    let Ok(path) = env_path() else {
        return (false, None);
    };
    let lines = read_lines(&path);
    match line_index(&lines, key)
        .and_then(|i| lines[i].split_once('=').map(|(_, v)| v.trim().to_owned()))
    {
        Some(v) if !v.is_empty() => (true, Some(mask(&v))),
        _ => (false, None),
    }
}

pub(super) fn env_path() -> Result<PathBuf, String> {
    let home = std::env::var("REGENT_HOME").map_err(|_| "REGENT_HOME is not set".to_owned())?;
    Ok(PathBuf::from(home).join(".env"))
}

pub(super) fn read_lines(path: &PathBuf) -> Vec<String> {
    std::fs::read_to_string(path)
        .map(|s| {
            // Strip a leading UTF-8 BOM (editors / PowerShell often prepend one).
            // Without this the FIRST var is invisible to `line_index` — its line
            // starts with U+FEFF, which `trim_start` does not remove.
            s.strip_prefix('\u{feff}')
                .unwrap_or(&s)
                .lines()
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn write_lines(path: &PathBuf, lines: &[String]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let body = format!("{}\n", lines.join("\n"));
    std::fs::write(path, body).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(windows)]
    {
        // The 0600 equivalent: grant the ACTUAL process-token SID, then strip
        // inherited ACEs. USERNAME can name the desktop account while the
        // process runs under an AppContainer/sandbox identity; granting that
        // other account locked the writer itself out of the freshly written
        // file and made list/delete silently see an empty .env.
        if let Some(sid) = current_user_sid() {
            let mut cmd = std::process::Command::new("icacls");
            cmd.arg(path)
                .args(["/inheritance:r", "/grant:r", &format!("*{sid}:F")]);
            let _ = crate::infra::no_window::hide_std(&mut cmd).output();
        }
    }
    Ok(())
}

#[cfg(windows)]
fn current_user_sid() -> Option<String> {
    let mut cmd = std::process::Command::new("whoami");
    cmd.args(["/user", "/fo", "csv", "/nh"]);
    let output = crate::infra::no_window::hide_std(&mut cmd).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let row = String::from_utf8(output.stdout).ok()?;
    parse_whoami_sid(row.trim())
}

#[cfg(windows)]
fn parse_whoami_sid(row: &str) -> Option<String> {
    let row = row.trim().trim_matches('"');
    let (_, sid) = row.split_once("\",\"")?;
    let sid = sid.trim_matches('"').trim();
    sid.starts_with("S-1-").then(|| sid.to_owned())
}

pub(super) fn line_index(lines: &[String], key: &str) -> Option<usize> {
    lines
        .iter()
        .position(|l| l.trim_start().starts_with(&format!("{key}=")))
}

pub(super) fn mask(v: &str) -> String {
    let t = v.trim();
    if t.len() <= 4 {
        "****".into()
    } else {
        format!("****{}", &t[t.len() - 4..])
    }
}

#[cfg(test)]
#[path = "tests/env_file.rs"]
mod tests;
