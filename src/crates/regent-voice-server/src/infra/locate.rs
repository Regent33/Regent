//! Where the regent-deacon binary might live, most-preferred first. Kept in
//! step with regent-cli's `binaryCandidates` and the desktop app's
//! `deacon_candidates`: `REGENT_DEACON_PATH` override, this exe's sibling, the
//! NEWEST `target/{release,debug}` (walked up from both this exe and the cwd)
//! BEFORE PATH, then PATH. The caller spawns and health-probes each in order,
//! so a pinned-but-dead first entry (a stale override whose config schema
//! drifted) no longer strands the voice server on a dead pipe — the next
//! healthy binary wins.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub(crate) fn deacon_name() -> &'static str {
    if cfg!(windows) {
        "regent-deacon.exe"
    } else {
        "regent-deacon"
    }
}

/// Ordered, de-duplicated (case-insensitively on Windows) list of existing
/// regent-deacon binaries to spawn/health-probe in turn.
pub(crate) fn deacon_candidates() -> Vec<PathBuf> {
    let name = deacon_name();
    let mut out: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // 1. The override — FIRST in probe order, but no longer decisive.
    if let Ok(pinned) = std::env::var("REGENT_DEACON_PATH") {
        add(&mut out, &mut seen, PathBuf::from(pinned));
    }
    // 2. Sibling of this executable (the installed layout).
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        add(&mut out, &mut seen, dir.join(name));
    }
    // 3. Newest target/{release,debug} walked up from this exe AND the cwd —
    //    BEFORE PATH so a fresh repo build beats a stale install.
    for base in walk_bases() {
        if let Some(candidate) = newest_in_target(&base, name) {
            add(&mut out, &mut seen, candidate);
        }
    }
    // 4. PATH.
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            add(&mut out, &mut seen, dir.join(name));
        }
    }
    out
}

/// Every directory to look for a `target/` under: this exe's ancestors first
/// (an installed server finding a repo build), then the cwd's (`cargo run`).
fn walk_bases() -> Vec<PathBuf> {
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        bases.extend(dir.ancestors().map(PathBuf::from));
    }
    if let Ok(cwd) = std::env::current_dir() {
        bases.extend(cwd.ancestors().map(PathBuf::from));
    }
    bases
}

/// Push `candidate` if it exists on disk and a case-normalized form is not in.
fn add(out: &mut Vec<PathBuf>, seen: &mut HashSet<String>, candidate: PathBuf) {
    if candidate.exists() && seen.insert(norm_key(&candidate)) {
        out.push(candidate);
    }
}

/// Case-normalized (on Windows) canonical key for de-duplication.
fn norm_key(path: &Path) -> String {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = canonical.to_string_lossy().into_owned();
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    }
}

/// Newest `target/{release,debug}/<name>` under `base` by mtime. Release-first
/// order silently ran a stale release exe after every debug rebuild — voice
/// then missed fixes that were sitting in the newer binary.
fn newest_in_target(base: &Path, name: &str) -> Option<PathBuf> {
    ["release", "debug"]
        .into_iter()
        .filter_map(|profile| {
            let candidate = base.join("target").join(profile).join(name);
            let modified = std::fs::metadata(&candidate)
                .and_then(|meta| meta.modified())
                .ok()?;
            Some((modified, candidate))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, candidate)| candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    /// Both profiles built: the NEWER one must win regardless of profile order,
    /// so a debug rebuild is not shadowed by yesterday's release exe.
    #[test]
    fn the_newest_build_profile_wins_over_release_order() {
        let base = tempfile::tempdir().unwrap();
        let name = deacon_name();
        for profile in ["release", "debug"] {
            let dir = base.path().join("target").join(profile);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(name), b"binary").unwrap();
        }
        let stale = std::fs::File::options()
            .write(true)
            .open(base.path().join("target").join("release").join(name))
            .unwrap();
        stale
            .set_modified(SystemTime::now() - Duration::from_secs(3_600))
            .unwrap();

        assert_eq!(
            newest_in_target(base.path(), name),
            Some(base.path().join("target").join("debug").join(name))
        );
    }

    #[test]
    fn a_base_without_a_build_yields_nothing() {
        let base = tempfile::tempdir().unwrap();
        assert_eq!(newest_in_target(base.path(), deacon_name()), None);
    }
}
