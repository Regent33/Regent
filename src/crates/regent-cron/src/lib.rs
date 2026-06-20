//! regent-cron — prospective memory: future intentions that survive
//! compaction and restarts (canonical `agents/memory`, prospective tier).
//!
//! Clean-architecture internal layout: `domain/` (job/schedule entities,
//! the repository + runner contracts, errors), `application/` (the tick
//! scheduler with the Hermes hardening invariants), `infra/` (JSON job
//! store + file tick lock).
//!
//! Hardening invariants (ported from Hermes): a **hard timeout** bounds
//! every run (runaway loops cannot monopolize the scheduler); the catch-up
//! window is half the period clamped to [120 s, 2 h] (one-shots get 120 s
//! grace); a **tick lock** prevents duplicate ticks across processes; jobs
//! run with fresh context and no memory/review (the runner decides, the
//! composition root enforces).

pub mod application;
pub mod domain;
pub mod infra;

pub use application::scheduler::{Scheduler, SchedulerConfig};
pub use domain::contracts::{JobRepository, JobRunner, TickGuard};
pub use domain::entities::{CronJob, RunOutcome, RunStatus, Schedule};
pub use domain::errors::CronError;
pub use infra::fs_jobs::FsJobRepository;
