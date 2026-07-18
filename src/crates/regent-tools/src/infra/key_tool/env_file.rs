//! `$REGENT_HOME/.env` file primitives: owner-only writes, hot-apply to the
//! process env, and masked reads (the raw value is never returned).

use std::path::PathBuf;

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
    match env_path() {
        Ok(path) => apply_credential_lines(&read_lines(&path)),
        Err(_) => 0,
    }
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
            let _ = std::process::Command::new("icacls")
                .arg(path)
                .args(["/inheritance:r", "/grant:r", &format!("*{sid}:F")])
                .output();
        }
    }
    Ok(())
}

#[cfg(windows)]
fn current_user_sid() -> Option<String> {
    let output = std::process::Command::new("whoami")
        .args(["/user", "/fo", "csv", "/nh"])
        .output()
        .ok()?;
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
mod tests {
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

    #[cfg(windows)]
    #[test]
    fn parses_the_process_token_sid_from_whoami_csv() {
        assert_eq!(
            parse_whoami_sid("\"machine\\user\",\"S-1-5-21-1-2-3-1001\""),
            Some("S-1-5-21-1-2-3-1001".into())
        );
    }
}
