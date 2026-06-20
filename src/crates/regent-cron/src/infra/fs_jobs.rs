//! Filesystem job repository: `jobs.json` + a `.tick.lock` file whose
//! atomic `create_new` gives cross-process tick exclusion (the Hermes
//! `~/.hermes/cron/.tick.lock` pattern). Stale locks (>10 min, e.g. after
//! a crash) are broken with a warning.

use crate::domain::contracts::{JobRepository, TickGuard};
use crate::domain::entities::CronJob;
use crate::domain::errors::CronError;
use std::path::PathBuf;

const STALE_LOCK_SECS: u64 = 600;

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

    fn lock_path(&self) -> PathBuf {
        self.dir.join(".tick.lock")
    }

    fn break_stale_lock(&self) {
        let path = self.lock_path();
        let Ok(metadata) = std::fs::metadata(&path) else { return };
        let stale = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age.as_secs() > STALE_LOCK_SECS);
        if stale {
            tracing::warn!(lock = %path.display(), "breaking stale tick lock");
            let _ = std::fs::remove_file(&path);
        }
    }
}

impl JobRepository for FsJobRepository {
    fn load(&self) -> Result<Vec<CronJob>, CronError> {
        match std::fs::read_to_string(self.jobs_path()) {
            Ok(raw) => serde_json::from_str(&raw)
                .map_err(|e| CronError::Storage(format!("corrupt jobs.json: {e}"))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error.into()),
        }
    }

    fn save(&self, jobs: &[CronJob]) -> Result<(), CronError> {
        let raw = serde_json::to_string_pretty(jobs)
            .map_err(|e| CronError::Storage(e.to_string()))?;
        Ok(std::fs::write(self.jobs_path(), raw)?)
    }

    fn try_lock_tick(&self) -> Result<Option<TickGuard>, CronError> {
        self.break_stale_lock();
        let path = self.lock_path();
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => Ok(Some(TickGuard::new(move || {
                let _ = std::fs::remove_file(&path);
            }))),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}
