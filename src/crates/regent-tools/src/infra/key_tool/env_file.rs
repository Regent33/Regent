//! `$REGENT_HOME/.env` file primitives: owner-only writes, hot-apply to the
//! process env, and masked reads (the raw value is never returned).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

const ENV_LOCK_TIMEOUT: Duration = Duration::from_secs(10);
const ENV_LOCK_RETRY: Duration = Duration::from_millis(20);

/// Insert or replace `KEY=value` in `$REGENT_HOME/.env`, with the same
/// owner-only-permission write as key storage. For non-secret knobs too
/// (the deacon's `voice.set` uses it for REGENT_WHISPER_SIZE).
pub fn upsert_env_var(key: &str, value: &str) -> Result<(), String> {
    validate_env_pair(key, value)?;
    let path = env_path()?;
    with_env_lock(&path, || {
        upsert_env_var_at(&path, key, value)?;
        // Hot-apply while still locked: concurrent threads must leave the
        // process environment matching the last file writer too.
        // SAFETY: same set_var pattern the boot .env merge uses.
        unsafe { std::env::set_var(key, value) };
        Ok(())
    })
}

/// Swap the VALUES of two `.env` keys (both must exist) — the multi-key
/// "activate" primitive: the runtime always resolves the base slot first, so
/// swapping slot N into the base makes it the active key while keeping the
/// other stored. Hot-applies both to the process env.
pub fn swap_env_vars(a: &str, b: &str) -> Result<(), String> {
    validate_env_name(a)?;
    validate_env_name(b)?;
    let path = env_path()?;
    with_env_lock(&path, || {
        let (va, vb) = swap_env_vars_at(&path, a, b)?;
        // SAFETY: mirrors upsert_env_var's hot-apply.
        unsafe {
            std::env::set_var(a, &vb);
            std::env::set_var(b, &va);
        }
        Ok(())
    })
}

/// Remove `KEY=...` from `$REGENT_HOME/.env`. Returns whether a line existed.
pub fn remove_env_var(key: &str) -> Result<bool, String> {
    validate_env_name(key)?;
    let path = env_path()?;
    with_env_lock(&path, || {
        let removed = remove_env_var_at(&path, key)?;
        if removed {
            // SAFETY: mirrors upsert_env_var's hot-apply.
            unsafe { std::env::remove_var(key) };
        }
        Ok(removed)
    })
}

fn upsert_env_var_at(path: &Path, key: &str, value: &str) -> Result<(), String> {
    let mut lines = read_lines(path)?;
    // Normalize externally-created duplicates while setting: one key has one
    // active assignment, and a later delete cannot expose a hidden old value.
    lines.retain(|line| !is_key_line(line, key));
    lines.push(format!("{key}={value}"));
    write_lines(path, &lines)
}

fn swap_env_vars_at(path: &Path, a: &str, b: &str) -> Result<(String, String), String> {
    let mut lines = read_lines(path)?;
    let value_of = |key: &str| {
        lines
            .iter()
            .find(|line| is_key_line(line, key))
            .and_then(|line| line.split_once('='))
            .map(|(_, value)| value.to_owned())
    };
    let (va, vb) = match (value_of(a), value_of(b)) {
        (Some(va), Some(vb)) => (va, vb),
        _ => return Err(format!("both {a} and {b} must be set to swap")),
    };
    validate_env_pair(a, &vb)?;
    validate_env_pair(b, &va)?;
    lines.retain(|line| !is_key_line(line, a) && !is_key_line(line, b));
    lines.push(format!("{a}={vb}"));
    lines.push(format!("{b}={va}"));
    write_lines(path, &lines)?;
    Ok((va, vb))
}

fn remove_env_var_at(path: &Path, key: &str) -> Result<bool, String> {
    let mut lines = read_lines(path)?;
    let before = lines.len();
    lines.retain(|line| !is_key_line(line, key));
    let removed = lines.len() != before;
    if removed {
        write_lines(path, &lines)?;
    }
    Ok(removed)
}

/// Serialize the complete read-modify-publish transaction across Regent's
/// Rust processes and the Bun CLI. Both implementations atomically create the
/// same dotenv lock directory; atomic rename alone cannot prevent two writers
/// that read the same old snapshot from losing one another's keys.
fn with_env_lock<T>(
    path: &Path,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let guard = EnvLock::acquire(path)?;
    let result = operation();
    drop(guard);
    result
}

struct EnvLock {
    path: PathBuf,
}

impl EnvLock {
    fn acquire(env: &Path) -> Result<Self, String> {
        if let Some(parent) = env.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let lock = sibling_with_suffix(env, ".lock");
        let started = Instant::now();
        loop {
            match std::fs::create_dir(&lock) {
                Ok(()) => return Ok(Self { path: lock }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if started.elapsed() >= ENV_LOCK_TIMEOUT {
                        return Err(format!(
                            "timed out waiting for the Regent credential lock: {}; if no Regent +                             process is saving settings, remove that empty lock directory and retry",
                            lock.display()
                        ));
                    }
                    std::thread::sleep(ENV_LOCK_RETRY);
                }
                Err(error) => {
                    return Err(format!(
                        "cannot acquire Regent credential lock {}: {error}",
                        lock.display()
                    ));
                }
            }
        }
    }
}

impl Drop for EnvLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir(&self.path);
    }
}

fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
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
                apply_credential_lines(&read_lines(&path).unwrap_or_default())
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
        let credential_name = numbered_credential_base(key);
        if value.is_empty()
            || value.contains('\0')
            || !key
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            || key.is_empty()
            || !CRED_SUFFIXES.iter().any(|s| credential_name.ends_with(s))
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

fn numbered_credential_base(name: &str) -> &str {
    let Some((base, slot)) = name.rsplit_once('_') else {
        return name;
    };
    if super::catalog::canonical_slot(slot).is_some() {
        base
    } else {
        name
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
    // Read-only: an unreadable file reports "not set" rather than failing a
    // status probe. Nothing is written on this path, so there is nothing to lose.
    let lines = read_lines(&path).unwrap_or_default();
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

/// The `.env` lines, or an error — **never** an empty file standing in for one
/// we could not read.
///
/// A missing file is genuinely empty: that is the first save on a fresh
/// install, and it must succeed. Every OTHER error is fatal to the caller,
/// because the write path is read-modify-publish: swallowing a permission or
/// I/O failure here produced an empty `Vec`, the caller pushed its one new key
/// onto it, and `write_lines` atomically replaced the user's entire credential
/// file with that single line. Silent, total, and indistinguishable from a
/// successful save. `unwrap_or_default()` on a read whose result is about to be
/// written back is the whole bug.
pub(super) fn read_lines(path: &Path) -> Result<Vec<String>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    // Strip a leading UTF-8 BOM (editors / PowerShell often prepend one).
    // Without this the FIRST var is invisible to `line_index` — its line
    // starts with U+FEFF, which `trim_start` does not remove.
    Ok(text
        .strip_prefix('\u{feff}')
        .unwrap_or(&text)
        .lines()
        .map(str::to_owned)
        .collect())
}

pub(super) fn write_lines(path: &Path, lines: &[String]) -> Result<(), String> {
    write_lines_with(path, lines, protect_owner_only)
}

fn write_lines_with(
    path: &Path,
    lines: &[String],
    protect: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let body = format!("{}\n", lines.join("\n"));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(".env");
    let temporary = path.with_file_name(format!(
        ".{file_name}.tmp.{}.{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));

    let result = (|| {
        // Publish no secret bytes until the temporary file has owner-only
        // permissions. The protected DACL/mode follows the same-file rename.
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(|e| e.to_string())?;
        protect(&temporary)?;
        file.write_all(body.as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        atomic_replace(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn protect_owner_only(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("cannot protect {}: {e}", path.display()))?;
    }
    #[cfg(windows)]
    {
        // The 0600 equivalent: grant the ACTUAL process-token SID, then strip
        // inherited ACEs. USERNAME can name the desktop account while the
        // process runs under an AppContainer/sandbox identity; granting that
        // other account locked the writer itself out of the freshly written
        // file and made list/delete silently see an empty .env.
        let sid = current_user_sid()?;
        let mut cmd = std::process::Command::new("icacls");
        cmd.arg(path)
            .args(["/inheritance:r", "/grant:r", &format!("*{sid}:F")]);
        let output = crate::infra::no_window::hide_std(&mut cmd)
            .output()
            .map_err(|e| format!("cannot run icacls for {}: {e}", path.display()))?;
        if !output.status.success() {
            return Err(format!(
                "cannot protect {} with icacls: {}",
                path.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn current_user_sid() -> Result<String, String> {
    let mut cmd = std::process::Command::new("whoami");
    cmd.args(["/user", "/fo", "csv", "/nh"]);
    let output = crate::infra::no_window::hide_std(&mut cmd)
        .output()
        .map_err(|e| format!("cannot discover the process identity: {e}"))?;
    if !output.status.success() {
        return Err("cannot discover the process identity with whoami".to_owned());
    }
    let row = String::from_utf8(output.stdout)
        .map_err(|_| "whoami returned a non-UTF-8 identity".to_owned())?;
    parse_whoami_sid(row.trim()).ok_or_else(|| "whoami returned no process SID".to_owned())
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    std::fs::rename(source, destination).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, new_name: *const u16, flags: u32) -> i32;
    }

    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: both pointers reference NUL-terminated UTF-16 buffers that live
    // for the duration of this call. REPLACE_EXISTING keeps replacement one
    // filesystem operation rather than deleting the live file first.
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}

fn validate_env_name(key: &str) -> Result<(), String> {
    if key.is_empty()
        || !key.as_bytes()[0].is_ascii_uppercase()
        || !key
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err("environment name must be an UPPER_SNAKE identifier".to_owned());
    }
    Ok(())
}

fn validate_env_pair(key: &str, value: &str) -> Result<(), String> {
    validate_env_name(key)?;
    if value.contains(['\n', '\r', '\0']) {
        return Err("environment value must be one line and cannot contain NUL bytes".to_owned());
    }
    Ok(())
}

#[cfg(windows)]
fn parse_whoami_sid(row: &str) -> Option<String> {
    let row = row.trim().trim_matches('"');
    let (_, sid) = row.split_once("\",\"")?;
    let sid = sid.trim_matches('"').trim();
    sid.starts_with("S-1-").then(|| sid.to_owned())
}

pub(super) fn line_index(lines: &[String], key: &str) -> Option<usize> {
    lines.iter().position(|line| is_key_line(line, key))
}

/// Does this line assign `key`? Must accept exactly what the LOADER accepts.
///
/// `apply_credential_lines` splits on the first `=` and then trims the name, so
/// `ANTHROPIC_API_KEY =secret` is a live assignment. This predicate required
/// `=` to touch the key, so it did not match that line — and it is what
/// `remove_env_var` and the duplicate-normalising `upsert` use to find lines.
/// The result: removing a key left the spaced copy in place and the deleted
/// credential kept loading on the next boot. A writer that recognises less
/// than its loader does not delete, it hides.
fn is_key_line(line: &str, key: &str) -> bool {
    line.trim_start()
        .strip_prefix(key)
        .is_some_and(|tail| tail.trim_start().starts_with('='))
}

pub(super) fn mask(v: &str) -> String {
    let t = v.trim();
    if t.chars().count() <= 4 {
        "****".into()
    } else {
        let tail: String = t
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("****{tail}")
    }
}

#[cfg(test)]
#[path = "tests/env_file.rs"]
mod tests;
