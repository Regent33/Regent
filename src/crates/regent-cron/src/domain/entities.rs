use crate::domain::errors::CronError;
use chrono::TimeZone;
use serde::{Deserialize, Serialize};

/// Seconds since the epoch, as the scheduler counts them.
fn now_epoch() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or_default()
}

/// `"09:30"`, `"9:30"`, `"9:30 pm"`, `"9pm"` → (hour, minute), 24h. Accepting
/// the am/pm forms is the difference between a model writing a schedule the
/// user asked for and one it had to convert in its head.
fn parse_hhmm(text: &str, raw: &str) -> Result<(u8, u8), CronError> {
    let bad = || CronError::InvalidSchedule(raw.to_owned());
    let lower = text.trim().to_ascii_lowercase();
    let (clock, meridiem) = if let Some(rest) = lower.strip_suffix("pm") {
        (rest.trim_end(), Some(12))
    } else if let Some(rest) = lower.strip_suffix("am") {
        (rest.trim_end(), Some(0))
    } else {
        (lower.as_str(), None)
    };
    let (h, m) = clock.split_once(':').unwrap_or((clock, "0"));
    let (mut hour, minute): (u8, u8) = (
        h.trim().parse().map_err(|_| bad())?,
        m.trim().parse().map_err(|_| bad())?,
    );
    if let Some(shift) = meridiem {
        // 12am = 00:00 and 12pm = 12:00 — the one hour where +12 is wrong.
        // Only with an explicit meridiem: bare "12:30" is 24h noon.
        if hour == 12 {
            hour = 0;
        }
        hour += shift;
    }
    if hour > 23 || minute > 59 {
        return Err(bad());
    }
    Ok((hour, minute))
}

/// Supported schedule shapes. Full 5-field cron expressions and natural
/// phrases ("every monday 9am") are deferred — these cover the headline
/// cases (periodic reports, nightly jobs, reminders).
///
/// Wall-clock shapes (`daily` / `once`) are **local time**, not UTC: a user
/// who says "8pm" means 8pm where they are, and a UTC reading fired those
/// jobs `offset` hours off with no error to notice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Schedule {
    /// `"30m"`, `"2h"`, `"1d"` — fixed period.
    Every { seconds: u64 },
    /// `"daily 09:30"` — once per day at local wall time.
    Daily { hour: u8, minute: u8 },
    /// Fires once at the given epoch, then the job disables itself.
    OneShot { at_epoch: f64 },
}

impl Schedule {
    pub fn parse(raw: &str) -> Result<Self, CronError> {
        let trimmed = raw.trim();
        if let Some(rest) = trimmed.strip_prefix("daily ") {
            let (hour, minute) = parse_hhmm(rest, raw)?;
            return Ok(Self::Daily { hour, minute });
        }
        // `once HH:MM` — the next local occurrence, then the job retires. This
        // is the "do this at 8pm" case; making the caller compute a Unix epoch
        // for it was the whole reason one-shots were unusable.
        if let Some(rest) = trimmed.strip_prefix("once ") {
            let (hour, minute) = parse_hhmm(rest, raw)?;
            let at_epoch = Self::Daily { hour, minute }
                .next_after(now_epoch())
                .ok_or_else(|| CronError::InvalidSchedule(raw.to_owned()))?;
            return Ok(Self::OneShot { at_epoch });
        }
        if let Some(epoch) = trimmed.strip_prefix('@') {
            let at_epoch = epoch
                .parse()
                .map_err(|_| CronError::InvalidSchedule(raw.to_owned()))?;
            return Ok(Self::OneShot { at_epoch });
        }
        let (digits, unit) = trimmed.split_at(trimmed.len().saturating_sub(1));
        let value: u64 = digits
            .parse()
            .map_err(|_| CronError::InvalidSchedule(raw.to_owned()))?;
        let seconds = match unit {
            "s" => value,
            "m" => value * 60,
            "h" => value * 3_600,
            "d" => value * 86_400,
            _ => return Err(CronError::InvalidSchedule(raw.to_owned())),
        };
        if seconds == 0 {
            return Err(CronError::InvalidSchedule(raw.to_owned()));
        }
        Ok(Self::Every { seconds })
    }

    /// The next fire time strictly after `now`. None = exhausted one-shot.
    #[must_use]
    pub fn next_after(&self, now: f64) -> Option<f64> {
        match self {
            Self::Every { seconds } => Some(now + *seconds as f64),
            Self::Daily { hour, minute } => {
                // Local wall time: walk today then tomorrow, taking the first
                // instant strictly after `now`. `.earliest()` is None inside a
                // DST spring-forward gap — that day simply has no such clock
                // time, so fall through to the next one.
                let today = chrono::Local
                    .timestamp_opt(now as i64, 0)
                    .single()?
                    .date_naive();
                (0..=1).find_map(|days| {
                    let at = (today + chrono::Duration::days(days))
                        .and_hms_opt(u32::from(*hour), u32::from(*minute), 0)?
                        .and_local_timezone(chrono::Local)
                        .earliest()?
                        .timestamp() as f64;
                    (at > now).then_some(at)
                })
            }
            Self::OneShot { at_epoch } => (*at_epoch > now).then_some(*at_epoch),
        }
    }

    /// Period used for the catch-up clamp; None for one-shots.
    #[must_use]
    pub fn period_seconds(&self) -> Option<u64> {
        match self {
            Self::Every { seconds } => Some(*seconds),
            Self::Daily { .. } => Some(86_400),
            Self::OneShot { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    pub id: String,
    pub name: String,
    pub schedule: Schedule,
    /// The user-style prompt the job agent runs with fresh context.
    pub prompt: String,
    pub enabled: bool,
    pub last_run_at: Option<f64>,
    pub next_run_at: f64,
    pub created_at: f64,
    /// Which surface owns this job, as `"<platform>:<chat_id>"` (e.g.
    /// `"telegram:12345"`). `None` = the local deacon's own job.
    ///
    /// One job store is shared by every process, so without this a job
    /// scheduled from a chat could be picked up by whichever scheduler won
    /// the tick lock — and its answer would land in a log file instead of
    /// the conversation that asked for it. Schedulers only run what they own
    /// (see `SchedulerConfig::owns`).
    #[serde(default)]
    pub target: Option<String>,
}

impl CronJob {
    pub fn new(
        name: impl Into<String>,
        schedule: Schedule,
        prompt: impl Into<String>,
        now: f64,
    ) -> Result<Self, CronError> {
        let next_run_at = schedule
            .next_after(now)
            .ok_or_else(|| CronError::InvalidSchedule("one-shot time is in the past".into()))?;
        Ok(Self {
            id: format!("job_{}", uuid::Uuid::new_v4().simple()),
            name: name.into(),
            schedule,
            prompt: prompt.into(),
            enabled: true,
            last_run_at: None,
            next_run_at,
            created_at: now,
            target: None,
        })
    }

    /// Bind the job to the surface that created it (see [`CronJob::target`]).
    #[must_use]
    pub fn for_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Ok,
    Failed,
    TimedOut,
    /// Missed beyond the catch-up window — skipped forward without running.
    SkippedCatchup,
}

#[derive(Debug, Clone)]
pub struct RunOutcome {
    pub job_id: String,
    pub job_name: String,
    pub status: RunStatus,
    pub summary: String,
}

#[cfg(test)]
#[path = "tests/entities.rs"]
mod tests;
