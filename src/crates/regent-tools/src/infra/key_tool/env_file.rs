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
    let value_of = |line: &str| line.splitn(2, '=').nth(1).unwrap_or("").to_owned();
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
        // The 0600 equivalent: strip inherited ACEs, grant only the current
        // user. Best-effort, same as the unix branch.
        let user = std::env::var("USERNAME").unwrap_or_default();
        if !user.is_empty() {
            let _ = std::process::Command::new("icacls")
                .arg(path)
                .args(["/inheritance:r", "/grant:r", &format!("{user}:F")])
                .output();
        }
    }
    Ok(())
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
}
