//! Filesystem job repository: `jobs.json` + a `.tick.lock` file whose
//! atomic `create_new` gives cross-process tick exclusion (the
//! `~/.regent/cron/.tick.lock` pattern). Stale locks (>10 min, e.g. after
//! a crash) are broken with a warning.

use crate::domain::contracts::{JobRepository, TickGuard};
use crate::domain::entities::CronJob;
use crate::domain::errors::CronError;
use std::path::PathBuf;

const STALE_LOCK_SECS: u64 = 600;
// Jobs-lock holds are milliseconds (one load+save), so staleness and the
// acquire wait are both short.
const STALE_JOBS_LOCK_SECS: u64 = 60;
const JOBS_LOCK_WAIT_SECS: u64 = 10;

pub struct FsJobRepository {
    dir: PathBuf,
}

impl FsJobRepository {
    pub fn new(dir: impl Into<PathBuf>) -> Result<Self, CronError> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    fn jobs_path(&self) -> PathBuf {
        self.dir.join("jobs.json")
    }

    fn bak_path(&self) -> PathBuf {
        self.dir.join("jobs.json.bak")
    }

    fn lock_path(&self) -> PathBuf {
        self.dir.join(".tick.lock")
    }

    fn break_stale(path: &PathBuf, stale_secs: u64, label: &str) {
        let Ok(metadata) = std::fs::metadata(path) else {
            return;
        };
        let stale = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age.as_secs() > stale_secs);
        if stale {
            tracing::warn!(lock = %path.display(), "breaking stale {label} lock");
            let _ = std::fs::remove_file(path);
        }
    }

    /// Cross-process mutual exclusion for load-mutate-save on `jobs.json`.
    /// Blocks (bounded) until the lock is free — mutations are short, so a
    /// waiter is almost always let in within milliseconds.
    fn lock_jobs(&self) -> Result<TickGuard, CronError> {
        let path = self.dir.join(".jobs.lock");
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_secs(JOBS_LOCK_WAIT_SECS);
        loop {
            Self::break_stale(&path, STALE_JOBS_LOCK_SECS, "jobs");
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => {
                    let release = path.clone();
                    return Ok(TickGuard::new(move || {
                        let _ = std::fs::remove_file(&release);
                    }));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if std::time::Instant::now() >= deadline {
                        return Err(CronError::Storage(
                            "timed out waiting for the jobs.json lock".into(),
                        ));
                    }
                    // ponytail: blocking sleep in a sync trait fn; holds are ms,
                    // contention is one tick vs one CLI command
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
}

impl JobRepository for FsJobRepository {
    fn load(&self) -> Result<Vec<CronJob>, CronError> {
        match std::fs::read_to_string(self.jobs_path()) {
            Ok(raw) => match serde_json::from_str(&raw) {
                Ok(jobs) => Ok(jobs),
                // Corrupt file (torn write, disk full): fall back to the last
                // good .bak; failing that, warn and start empty — the corrupt
                // file stays on disk for manual recovery, and one bad byte
                // must not brick every cron surface.
                Err(error) => {
                    tracing::warn!(%error, "corrupt jobs.json; trying jobs.json.bak");
                    let bak = std::fs::read_to_string(self.bak_path())
                        .ok()
                        .and_then(|raw| serde_json::from_str(&raw).ok());
                    match bak {
                        Some(jobs) => Ok(jobs),
                        None => {
                            tracing::warn!("no usable jobs.json.bak; starting with no jobs");
                            Ok(Vec::new())
                        }
                    }
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error.into()),
        }
    }

    /// Write-temp-then-rename so a crash mid-write can't tear `jobs.json`;
    /// the previous good file is kept as `.bak` (the `load` fallback).
    fn save(&self, jobs: &[CronJob]) -> Result<(), CronError> {
        let raw =
            serde_json::to_string_pretty(jobs).map_err(|e| CronError::Storage(e.to_string()))?;
        let path = self.jobs_path();
        let tmp = self.dir.join("jobs.json.tmp");
        std::fs::write(&tmp, raw)?;
        if path.exists() {
            let _ = std::fs::copy(&path, self.bak_path());
        }
        Ok(std::fs::rename(&tmp, &path)?)
    }

    fn mutate(&self, f: &mut dyn FnMut(&mut Vec<CronJob>)) -> Result<(), CronError> {
        let _guard = self.lock_jobs()?;
        let mut jobs = self.load()?;
        f(&mut jobs);
        self.save(&jobs)
    }

    fn try_lock_tick(&self) -> Result<Option<TickGuard>, CronError> {
        Self::break_stale(&self.lock_path(), STALE_LOCK_SECS, "tick");
        let path = self.lock_path();
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => Ok(Some(TickGuard::new(move || {
                let _ = std::fs::remove_file(&path);
            }))),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}
