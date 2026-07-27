//! The v10 job-ledger tables (W1). Split from `schema.rs` for the file-size
//! rule and applied as a second batch by `Store::open` — same declarative,
//! `IF NOT EXISTS` treatment as the rest of the schema, so it needs no
//! numbered migration.

pub const JOBS_SCHEMA_SQL: &str = r#"
-- Job ledger (v10, W1): every unit of work that outlives the turn that started
-- it — background, cron, coding, delegated. Before this the background board was
-- a process-global `Vec` in memory: a restart dropped every running job silently
-- while the user had already been told "I'll report back".
--
-- `state` is the machine: queued -> running -> succeeded | failed | cancelled |
-- interrupted | inconclusive. `interrupted` exists so a job cut off by a restart
-- is never reported as either done or failed, and `inconclusive` is a LEGAL
-- terminal state — "the process ended and we cannot tell whether it worked" is
-- an honest answer that the old Done/Failed pair could not express.
--
-- The four completion facts are stored SEPARATELY and each is 'yes'|'no'|
-- 'unknown', because collapsing them into one boolean is what let a job that
-- produced nothing report success. `session_id` points at the background
-- transcript, so the evidence for any claim is retrievable.
CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    label TEXT NOT NULL,
    task TEXT NOT NULL DEFAULT '',
    idempotency_key TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'queued'
        CHECK (state IN ('queued', 'running', 'succeeded', 'failed',
                         'inconclusive', 'interrupted', 'cancelled', 'timed_out')),
    session_id TEXT,
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 1,
    deadline_at REAL,
    cancel_requested INTEGER NOT NULL DEFAULT 0,
    process_completed TEXT NOT NULL DEFAULT 'unknown'
        CHECK (process_completed IN ('yes', 'no', 'unknown')),
    artifact_produced TEXT NOT NULL DEFAULT 'unknown'
        CHECK (artifact_produced IN ('yes', 'no', 'unknown')),
    result_validated TEXT NOT NULL DEFAULT 'unknown'
        CHECK (result_validated IN ('yes', 'no', 'unknown')),
    outcome_achieved TEXT NOT NULL DEFAULT 'unknown'
        CHECK (outcome_achieved IN ('yes', 'no', 'unknown')),
    result TEXT,
    error TEXT,
    delivered_at REAL,
    created_at REAL NOT NULL,
    updated_at REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_jobs_state ON jobs(state, created_at);
-- Idempotency, enforced by the DB rather than by a scan: only ONE live job per
-- key can exist. The model re-fires `background_task` for the same work (the
-- doom-loop guard caught four repeats in a day); each twin left behind was a row
-- that reported "still running" forever after the real job finished.
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_idempotency
    ON jobs(idempotency_key) WHERE state IN ('queued', 'running');

-- One row per try. Kept separate from `jobs` so a retry never overwrites the
-- record of what happened the first time.
CREATE TABLE IF NOT EXISTS job_attempts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id TEXT NOT NULL,
    attempt INTEGER NOT NULL,
    session_id TEXT,
    started_at REAL NOT NULL,
    ended_at REAL,
    outcome TEXT,
    error TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_job_attempts_job ON job_attempts(job_id, attempt);

-- What a job actually produced. An empty set with state='succeeded' is the
-- signal that `artifact_produced` must not be claimed.
CREATE TABLE IF NOT EXISTS job_artifacts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id TEXT NOT NULL,
    path TEXT NOT NULL,
    recorded_at REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_job_artifacts_job ON job_artifacts(job_id);
"#;
