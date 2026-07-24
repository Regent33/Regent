//! REGENT_HOME resolution and the child-env contract, mirroring regent-cli's
//! spawn.ts: `REGENT_HOME` is forced, `$REGENT_HOME/.env` is merged (the real
//! environment wins), `REGENT_NOW` hands the clock-less deacon the wall-clock,
//! and desktop control defaults on.

use std::path::{Path, PathBuf};

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
        // Strip a leading UTF-8 BOM (editors/PowerShell add one) — otherwise the
        // FIRST var is exported with a `\u{feff}` prefix in its name and every
        // `std::env::var("NAME")` lookup misses it (e.g. REGENT_API_KEY).
        let dotenv = dotenv.strip_prefix('\u{feff}').unwrap_or(&dotenv);
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
    // Computer use default-on, matching regent-cli (spawn.ts) and the voice
    // server — all Regent front-ends in unison. Desktop chat gates every
    // mutating action through the approval.request UI; REGENT_COMPUTER_USE=0
    // in the real env or .env disables (the .env merge above already applied,
    // and a real-env value short-circuits here).
    if std::env::var("REGENT_COMPUTER_USE").is_err()
        && !pairs.iter().any(|(k, _)| k == "REGENT_COMPUTER_USE")
    {
        pairs.push(("REGENT_COMPUTER_USE".to_string(), "1".to_string()));
    }
    pairs
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
