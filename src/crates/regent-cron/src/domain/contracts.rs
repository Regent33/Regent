use crate::domain::entities::CronJob;
use crate::domain::errors::CronError;
use async_trait::async_trait;

/// RAII tick-lock guard: dropping it releases the lock. Boxed closure so
/// implementations stay free to choose their mechanism (file, advisory…).
pub struct TickGuard(Option<Box<dyn FnOnce() + Send>>);

impl TickGuard {
    #[must_use]
    pub fn new(release: impl FnOnce() + Send + 'static) -> Self {
        Self(Some(Box::new(release)))
    }
}

impl Drop for TickGuard {
    fn drop(&mut self) {
        if let Some(release) = self.0.take() {
            release();
        }
    }
}

/// Persistence contract for the job store.
pub trait JobRepository: Send + Sync {
    fn load(&self) -> Result<Vec<CronJob>, CronError>;

    fn save(&self, jobs: &[CronJob]) -> Result<(), CronError>;

    /// Non-blocking: None when another process holds the tick lock.
    fn try_lock_tick(&self) -> Result<Option<TickGuard>, CronError>;
}

/// Executes one due job and returns a short summary. The composition root
/// decides what a run is (a fresh agent with cron source, no memory/review).
#[async_trait]
pub trait JobRunner: Send + Sync {
    async fn run(&self, job: &CronJob) -> Result<String, CronError>;
}
