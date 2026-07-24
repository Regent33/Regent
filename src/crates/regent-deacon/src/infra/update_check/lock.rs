//! Cross-process best-effort lock for one update request per Regent home.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const STALE_AFTER: Duration = Duration::from_secs(5 * 60);

pub(super) struct CheckLock {
    path: PathBuf,
}

impl CheckLock {
    pub(super) fn acquire(home: &Path) -> Option<Self> {
        let dir = home.join("update");
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join("check.lock");
        for _ in 0..2 {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    let _ = writeln!(file, "{}", std::process::id());
                    return Some(Self { path });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if !is_stale(&path) {
                        return None;
                    }
                    let _ = std::fs::remove_file(&path);
                }
                Err(_) => return None,
            }
        }
        None
    }
}

impl Drop for CheckLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn is_stale(path: &Path) -> bool {
    let Ok(modified) = std::fs::metadata(path).and_then(|meta| meta.modified()) else {
        return true;
    };
    SystemTime::now()
        .duration_since(modified)
        .is_ok_and(|age| age >= STALE_AFTER)
}
