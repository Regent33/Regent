use crate::domain::errors::CronError;
use serde::{Deserialize, Serialize};

/// Supported schedule shapes. Full 5-field cron expressions and natural
/// phrases ("every monday 9am") are deferred — these three cover the
/// headline cases (periodic reports, nightly jobs, reminders).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Schedule {
    /// `"30m"`, `"2h"`, `"1d"` — fixed period.
    Every { seconds: u64 },
    /// `"daily 09:30"` — once per day at UTC wall time.
    Daily { hour: u8, minute: u8 },
    /// Fires once at the given epoch, then the job disables itself.
    OneShot { at_epoch: f64 },
}

impl Schedule {
    pub fn parse(raw: &str) -> Result<Self, CronError> {
        let trimmed = raw.trim();
        if let Some(rest) = trimmed.strip_prefix("daily ") {
            let (hour, minute) = rest
                .split_once(':')
                .ok_or_else(|| CronError::InvalidSchedule(raw.to_owned()))?;
            let (hour, minute): (u8, u8) = (
                hour.parse()
                    .map_err(|_| CronError::InvalidSchedule(raw.to_owned()))?,
                minute
                    .parse()
                    .map_err(|_| CronError::InvalidSchedule(raw.to_owned()))?,
            );
            if hour > 23 || minute > 59 {
                return Err(CronError::InvalidSchedule(raw.to_owned()));
            }
            return Ok(Self::Daily { hour, minute });
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
                let day = 86_400.0;
                let target = f64::from(*hour) * 3_600.0 + f64::from(*minute) * 60.0;
                let midnight = (now / day).floor() * day;
                let today = midnight + target;
                Some(if today > now { today } else { today + day })
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
        })
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
mod tests {
    use super::*;

    #[test]
    fn parses_supported_formats_and_rejects_garbage() {
        assert_eq!(
            Schedule::parse("30m").unwrap(),
            Schedule::Every { seconds: 1_800 }
        );
        assert_eq!(
            Schedule::parse("2h").unwrap(),
            Schedule::Every { seconds: 7_200 }
        );
        assert_eq!(
            Schedule::parse("1d").unwrap(),
            Schedule::Every { seconds: 86_400 }
        );
        assert_eq!(
            Schedule::parse("daily 09:30").unwrap(),
            Schedule::Daily {
                hour: 9,
                minute: 30
            }
        );
        assert_eq!(
            Schedule::parse("@1000.5").unwrap(),
            Schedule::OneShot { at_epoch: 1000.5 }
        );
        for bad in [
            "",
            "0m",
            "5x",
            "daily 25:00",
            "daily nine",
            "@soon",
            "monday",
        ] {
            assert!(Schedule::parse(bad).is_err(), "should reject {bad}");
        }
    }

    #[test]
    fn next_after_semantics() {
        let every = Schedule::Every { seconds: 60 };
        assert_eq!(every.next_after(100.0), Some(160.0));

        let daily = Schedule::Daily {
            hour: 0,
            minute: 10,
        };
        // 600s into the day → today 00:10 already passed at 700s.
        assert_eq!(daily.next_after(700.0), Some(86_400.0 + 600.0));
        assert_eq!(daily.next_after(100.0), Some(600.0));

        let oneshot = Schedule::OneShot { at_epoch: 500.0 };
        assert_eq!(oneshot.next_after(100.0), Some(500.0));
        assert_eq!(oneshot.next_after(600.0), None);
    }
}
