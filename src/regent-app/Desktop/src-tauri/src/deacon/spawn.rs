//! Spawn regent-deacon as a hidden stdio child. Env contract mirrors
//! regent-cli's spawn.ts: `REGENT_HOME` is forced, `$REGENT_HOME/.env` is
//! merged (real environment wins), and `REGENT_NOW` hands the clock-less deacon
//! the current wall-clock. Binary resolution is ported from the voice server's
//! `find_deacon` so all front-ends agree on the search order.

use super::rpc::DeaconRpc;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::{Child, Command};

/// Spawn the deacon and return a connected client plus the child handle. The
/// child dies with this process (`kill_on_drop`); `notify` receives every
/// streamed notification line.
pub async fn spawn(
    notify: impl Fn(Value) + Send + 'static,
) -> Result<(Arc<DeaconRpc>, Child), String> {
    let deacon = find_deacon().ok_or_else(|| {
        "regent-deacon binary not found (set REGENT_DEACON_PATH or build it with \
         `cargo build -p regent-deacon`)"
            .to_string()
    })?;
    let home = regent_home();
    std::fs::create_dir_all(&home)
        .map_err(|e| format!("create REGENT_HOME {}: {e}", home.display()))?;

    let mut cmd = Command::new(&deacon);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Inherit stderr: the deacon logs there (stdout is the JSON-RPC stream),
        // so its logs stay visible in the dev console.
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    apply_env(&mut cmd, &home);
    // CREATE_NO_WINDOW — a hidden child must never flash a console window; the
    // repo has been burned by visible/focus-stealing consoles (see CHANGELOG).
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawn deacon {}: {e}", deacon.display()))?;
    let (Some(stdout), Some(stdin)) = (child.stdout.take(), child.stdin.take()) else {
        return Err("deacon stdio pipes were not created".into());
    };
    Ok((DeaconRpc::from_io(stdout, stdin, notify), child))
}

/// `$REGENT_HOME`, else `%USERPROFILE%\.regent` (`$HOME/.regent` off Windows).
pub(crate) fn regent_home() -> PathBuf {
    if let Ok(h) = std::env::var("REGENT_HOME") {
        return PathBuf::from(h);
    }
    let user = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    PathBuf::from(user).join(".regent")
}

/// The child-env contract as pure pairs (usable by tokio and std Commands
/// alike): `REGENT_HOME` forced, `$REGENT_HOME/.env` merged with the real
/// environment winning, `REGENT_NOW` set.
pub(crate) fn merged_env(home: &Path) -> Vec<(String, String)> {
    let mut pairs = vec![("REGENT_HOME".to_string(), home.display().to_string())];
    if let Ok(dotenv) = std::fs::read_to_string(home.join(".env")) {
        for raw in dotenv.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            // REGENT_HOME is forced above; never let .env override the real env.
            if key.is_empty() || key == "REGENT_HOME" || std::env::var(key).is_ok() {
                continue;
            }
            pairs.push((key.to_string(), value.to_string()));
        }
    }
    pairs.push(("REGENT_NOW".to_string(), wall_clock_now()));
    pairs
}

fn apply_env(cmd: &mut Command, home: &Path) {
    cmd.envs(merged_env(home));
}

/// Current wall-clock as `YYYY-MM-DD HH:MM:SS UTC`, computed with std-only
/// arithmetic. spawn.ts hands the deacon a LOCAL string via `toLocaleString()`;
/// producing a local-tz string in Rust needs a time crate, which this thin
/// bridge deliberately avoids — UTC still answers the agent's date/time.
fn wall_clock_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (hh, mm, ss) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Howard Hinnant's civil-from-days (proleptic Gregorian).
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02} UTC")
}

fn deacon_name() -> &'static str {
    if cfg!(windows) {
        "regent-deacon.exe"
    } else {
        "regent-deacon"
    }
}

/// Locate the regent-deacon binary — ported from the voice server so both
/// front-ends agree: `REGENT_DEACON_PATH` override, then `target/{release,
/// debug}` walking up from the cwd and this exe, then PATH.
fn find_deacon() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("REGENT_DEACON_PATH") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let name = deacon_name();
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        bases.extend(cwd.ancestors().map(PathBuf::from));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            bases.extend(dir.ancestors().map(PathBuf::from));
        }
    }
    for base in &bases {
        for profile in ["release", "debug"] {
            let cand = base.join("target").join(profile).join(name);
            if cand.exists() {
                return Some(cand);
            }
        }
    }
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|d| d.join(name))
        .find(|c| c.exists())
}
