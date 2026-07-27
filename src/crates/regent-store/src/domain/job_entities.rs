//! Job-ledger row types (W1). Split from `entities.rs` for the file-size rule;
//! re-exported from `domain::entities` so callers see one entity namespace.

/// A durable job — any unit of work that outlives the turn that started it.
///
/// The four completion facts are deliberately separate `yes`/`no`/`unknown`
/// strings rather than one boolean: a job whose process ended is not the same
/// claim as a job that produced something, and neither is the same as a job
/// that achieved what was asked. Collapsing them is what let jobs that
/// delivered nothing report success.
#[derive(Debug, Clone, PartialEq)]
pub struct JobRow {
    pub id: String,
    /// `background` | `cron` | `coding` | `delegated`.
    pub kind: String,
    pub label: String,
    pub task: String,
    /// Only one live job may hold a given key (enforced by a partial unique
    /// index over `queued`/`running`).
    pub idempotency_key: String,
    /// `queued` | `running` | `succeeded` | `failed` | `cancelled` |
    /// `interrupted` | `inconclusive`.
    pub state: String,
    /// The transcript this job ran in — the evidence for any claim about it.
    pub session_id: Option<String>,
    pub attempts: i64,
    pub max_attempts: i64,
    pub deadline_at: Option<f64>,
    pub cancel_requested: bool,
    pub process_completed: String,
    pub artifact_produced: String,
    pub result_validated: String,
    pub outcome_achieved: String,
    pub result: Option<String>,
    pub error: Option<String>,
    /// When its outcome was relayed to the user. `None` = not yet reported.
    pub delivered_at: Option<f64>,
    pub created_at: f64,
    pub updated_at: f64,
}

/// One try at a job. Separate from [`JobRow`] so a retry never overwrites the
/// record of what happened the time before.
#[derive(Debug, Clone, PartialEq)]
pub struct JobAttemptRow {
    pub job_id: String,
    pub attempt: i64,
    pub session_id: Option<String>,
    pub started_at: f64,
    pub ended_at: Option<f64>,
    pub outcome: Option<String>,
    pub error: Option<String>,
}
