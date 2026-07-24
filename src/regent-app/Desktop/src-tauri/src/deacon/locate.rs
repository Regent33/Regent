//! Where the regent-deacon binary might live, most-preferred first. Ported to
//! agree with regent-cli's `binaryCandidates` and the voice server's search:
//! REGENT_DEACON_PATH override, this exe's sibling, the NEWEST
//! target/{release,debug} (walked up from both this exe and the cwd) BEFORE
//! PATH, then PATH. The caller spawns and health-probes each in order, so a
//! pinned-but-dead first entry (an old override whose config schema drifted) no
//! longer wins the app a dead pipe — the next healthy binary does.

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
    if let Ok(p) = std::env::var("REGENT_DEACON_PATH") {
        add(&mut out, &mut seen, PathBuf::from(p));
    }
    // 2. Sibling of this executable (the installed layout).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            add(&mut out, &mut seen, dir.join(name));
        }
    }
    // 3. Newest target/{release,debug} walked up from this exe AND the cwd —
    //    BEFORE PATH so a fresh repo build beats a stale install. The cwd walk
    //    covers `bun tauri dev`; the exe walk lets an installed app find it.
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            bases.extend(dir.ancestors().map(PathBuf::from));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        bases.extend(cwd.ancestors().map(PathBuf::from));
    }
    for base in &bases {
        if let Some(cand) = newest_in_target(base, name) {
            add(&mut out, &mut seen, cand);
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

/// Push `p` if it exists on disk and a case-normalized form is not already in.
fn add(out: &mut Vec<PathBuf>, seen: &mut HashSet<String>, p: PathBuf) {
    if p.exists() && seen.insert(norm_key(&p)) {
        out.push(p);
    }
}

/// Case-normalized (on Windows) canonical key for de-duplication.
fn norm_key(p: &Path) -> String {
    let canon = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let s = canon.to_string_lossy().into_owned();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

/// Newest `target/{release,debug}/<name>` under `base` by mtime. Release-first
/// order silently ran a stale release exe after every debug rebuild — the app
/// then missed fixes that were sitting in the newer binary.
pub(crate) fn newest_in_target(base: &Path, name: &str) -> Option<PathBuf> {
    ["release", "debug"]
        .into_iter()
        .filter_map(|profile| {
            let cand = base.join("target").join(profile).join(name);
            let modified = std::fs::metadata(&cand).and_then(|m| m.modified()).ok()?;
            Some((modified, cand))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, cand)| cand)
}
