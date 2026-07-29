//! The job ledger — one durable record per unit of work that outlives the turn
//! that started it.
//!
//! Its own crate rather than the deacon's, because the deacon is not the only
//! process that runs work. The gateway schedules cron jobs for chat surfaces
//! over the *same* `state.db`, and could not see a ledger that lived above it
//! in the dependency graph — so its executions were unrecorded and invisible to
//! `regent jobs`. The gate on this work says one ledger for all work, not a
//! second one per surface; that is only possible from here.

pub mod application;
pub mod domain;

pub use application::{
    CRON_BUDGET_SECS, CRON_WATCHDOG_SECS, JobLedger, JobLimits, LedgerCronRunner, render_updates,
};
pub use domain::job::{Completion, Fact, JobState, StopReason};
