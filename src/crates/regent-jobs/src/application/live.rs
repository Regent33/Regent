//! Reading and signalling on a live job — the accessors that neither claim nor
//! close one. Split from `mod.rs` (file-size rule); same `impl JobLedger`.

use super::ledger::JobLedger;
use regent_store::domain::job_entities::JobRow;

impl JobLedger {
    /// Whether cancellation has been requested — polled by a running job.
    pub fn cancel_requested(&self, id: &str) -> bool {
        matches!(self.store.job(id), Ok(Some(job)) if job.cancel_requested)
    }

    /// Renews this attempt's claim. A running job must call this on a timer, or
    /// the next deacon to boot will (correctly) treat it as abandoned — see
    /// `Store::interrupt_running_jobs`.
    pub fn heartbeat(&self, id: &str, attempt: i64) {
        if let Err(error) = self.store.heartbeat_job(id, attempt) {
            // Not fatal on its own: one missed renewal is well inside the
            // lease. Logged because a persistent failure ends with a healthy
            // job being reclaimed out from under itself.
            tracing::warn!(%error, job = id, "could not renew the job lease");
        }
    }

    /// Jobs whose deadline has passed while still running.
    pub fn overdue(&self) -> Vec<JobRow> {
        let now = regent_store::infra::db::now_epoch();
        self.store
            .live_jobs()
            .unwrap_or_default()
            .into_iter()
            .filter(|job| job.deadline_at.is_some_and(|deadline| now >= deadline))
            .collect()
    }

    /// Everything still queued or running. The ledger already knew this; until
    /// `job.list` there was no way to *ask* — job state reached the user only by
    /// riding [`JobLedger::updates`] into their next turn.
    pub fn live(&self) -> Vec<JobRow> {
        self.store.live_jobs().unwrap_or_default()
    }

    /// Finished jobs nobody has been told about yet.
    ///
    /// The completion PUSH (`job.finished`) is a best-effort stdio notification
    /// rendered into client-local state, so a UI reload, a route change, or an
    /// app restart loses it — and `live()` only knows about queued/running
    /// work, so nothing could re-read "finished but unreported". The ledger has
    /// always had this set; it just had no way out. It self-empties: the next
    /// successful turn marks these delivered.
    pub fn undelivered(&self) -> Vec<JobRow> {
        self.store.undelivered_jobs().unwrap_or_default()
    }

    /// Requests cancellation. The running job notices on its next poll, which
    /// is why this returns "asked", not "stopped" — see `background_task_tool`.
    pub fn request_cancel(&self, id: &str) -> bool {
        self.store.request_job_cancel(id).unwrap_or(false)
    }

    pub fn record_artifact(&self, id: &str, path: &str) {
        if let Err(error) = self.store.record_job_artifact(id, path) {
            tracing::warn!(%error, job = id, "could not record job artifact");
        }
    }

    /// What the job produced. An empty set is the evidence that
    /// `artifact_produced` must not be claimed.
    pub fn artifacts(&self, id: &str) -> Vec<String> {
        self.store.job_artifacts(id).unwrap_or_default()
    }
}
