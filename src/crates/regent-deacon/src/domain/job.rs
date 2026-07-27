//! The job state machine and what "done" is allowed to mean (W1).
//!
//! The defect this exists to prevent: a job whose *process* returned was
//! reported to the user as finished work. Those are different claims. Here they
//! are four separate facts, and the terminal state is derived from them rather
//! than asserted — so the only way to reach `Succeeded` is to be able to say the
//! outcome was achieved.
//!
//! Pure: no store, no IO, no async. The runner decides which facts it can
//! honestly fill in; this module decides what those facts entitle it to claim.

use std::fmt;

/// What we know about one completion fact. `Unknown` is the honest default and
/// is never silently upgraded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fact {
    Yes,
    No,
    Unknown,
}

impl Fact {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Fact::Yes => "yes",
            Fact::No => "no",
            Fact::Unknown => "unknown",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Self {
        match raw {
            "yes" => Fact::Yes,
            "no" => Fact::No,
            _ => Fact::Unknown,
        }
    }
}

/// Where a job is. `Interrupted` and `Cancelled` are set by the runtime (a
/// restart, a user request); the other terminal states are derived from
/// [`Completion`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Failed,
    /// The process ended and we cannot tell whether the work landed. A legal
    /// answer, and far more useful than a guess in either direction.
    Inconclusive,
    /// Cut off mid-flight — a deacon restart, a crash. Not success, not
    /// failure. Retryable.
    Interrupted,
    Cancelled,
    /// Stopped for exceeding its deadline. Distinct from `Failed`: we know it
    /// did not finish in time, NOT that the work failed.
    TimedOut,
}

/// Why the RUNTIME stopped a job, as opposed to the job reporting its own
/// completion. These are facts about the process; none of them says anything
/// about whether the goal was achieved, so all of them leave
/// `outcome_achieved` unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Cancelled,
    TimedOut,
    Interrupted,
}

impl StopReason {
    #[must_use]
    pub fn state(self) -> JobState {
        match self {
            StopReason::Cancelled => JobState::Cancelled,
            StopReason::TimedOut => JobState::TimedOut,
            StopReason::Interrupted => JobState::Interrupted,
        }
    }
}

impl JobState {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            JobState::Queued => "queued",
            JobState::Running => "running",
            JobState::Succeeded => "succeeded",
            JobState::Failed => "failed",
            JobState::Inconclusive => "inconclusive",
            JobState::Interrupted => "interrupted",
            JobState::Cancelled => "cancelled",
            JobState::TimedOut => "timed_out",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Self {
        match raw {
            "running" => JobState::Running,
            "succeeded" => JobState::Succeeded,
            "failed" => JobState::Failed,
            "inconclusive" => JobState::Inconclusive,
            "interrupted" => JobState::Interrupted,
            "cancelled" => JobState::Cancelled,
            "timed_out" => JobState::TimedOut,
            _ => JobState::Queued,
        }
    }

    /// Whether the job is finished. `Interrupted` counts: that attempt is over,
    /// even though the job may be retried.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        !matches!(self, JobState::Queued | JobState::Running)
    }

    /// Whether another attempt is allowed to start from here. Only states
    /// where the work provably did not run to a conclusion: retrying a job
    /// that reported an outcome would redo work the user already has.
    ///
    /// NOTE: this must agree with the SQL in `start_job_attempt`, which is the
    /// enforcing copy. `Failed` is deliberately NOT here — a failed job has a
    /// recorded outcome; rerunning it is the caller's decision, not automatic.
    #[must_use]
    pub fn is_retryable(self) -> bool {
        matches!(self, JobState::Interrupted)
    }
}

impl fmt::Display for JobState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The four facts, stored and reasoned about separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Completion {
    /// The process ran to the end without dying.
    pub process_completed: Fact,
    /// Something concrete exists that the job produced.
    pub artifact_produced: Fact,
    /// That output was checked — a build passed, a test ran, a file parsed.
    pub result_validated: Fact,
    /// What the user actually asked for happened.
    pub outcome_achieved: Fact,
}

impl Completion {
    /// Nothing known. The starting point for any honest assessment.
    #[must_use]
    pub fn unknown() -> Self {
        Self {
            process_completed: Fact::Unknown,
            artifact_produced: Fact::Unknown,
            result_validated: Fact::Unknown,
            outcome_achieved: Fact::Unknown,
        }
    }

    /// The process died with an error.
    ///
    /// `outcome_achieved` stays UNKNOWN, not `No`: a job can write the file and
    /// then fall over while reporting, so an error proves the process failed —
    /// it does not prove the goal was missed. Claiming `No` here would be the
    /// same overreach as claiming `Yes` from a bare return, in the other
    /// direction.
    #[must_use]
    pub fn process_failed() -> Self {
        Self {
            process_completed: Fact::No,
            artifact_produced: Fact::Unknown,
            result_validated: Fact::Unknown,
            outcome_achieved: Fact::Unknown,
        }
    }

    /// The terminal state these facts justify — never more than they support.
    ///
    /// `Succeeded` requires being able to say the outcome was achieved. A job
    /// that merely ran to the end is `Inconclusive`, which is the honest answer
    /// and the one the old Done/Failed pair could not express.
    #[must_use]
    pub fn verdict(&self) -> JobState {
        match self.outcome_achieved {
            Fact::Yes => JobState::Succeeded,
            Fact::No => JobState::Failed,
            Fact::Unknown => JobState::Inconclusive,
        }
    }

    #[must_use]
    pub fn as_row(&self) -> [&'static str; 4] {
        [
            self.process_completed.as_str(),
            self.artifact_produced.as_str(),
            self.result_validated.as_str(),
            self.outcome_achieved.as_str(),
        ]
    }

    /// A caveat for the user-facing report, or `None` when the evidence backs
    /// the claim. This is what stops "done" from meaning "the process exited".
    #[must_use]
    pub fn caveat(&self) -> Option<&'static str> {
        match (self.outcome_achieved, self.result_validated) {
            (Fact::Yes, Fact::Yes) => None,
            (Fact::Yes, _) => Some("self-reported by the job; not independently verified"),
            (Fact::No, _) => None,
            (Fact::Unknown, _) if self.process_completed == Fact::Yes => {
                Some("the job ran to the end, but whether it achieved the goal is unverified")
            }
            (Fact::Unknown, _) => Some("no verified result — do not describe this as finished"),
        }
    }
}

#[cfg(test)]
#[path = "tests/job.rs"]
mod tests;
