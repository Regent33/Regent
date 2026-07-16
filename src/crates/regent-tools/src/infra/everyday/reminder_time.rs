//! Time resolution for the `reminder` tool: one-off fire times, local
//! formatting, and the local→UTC conversion for daily recurrences. Errors are
//! plain `String`s — `reminder.rs` wraps them into its tool error.

use chrono::{DateTime, Duration, Local, NaiveDateTime, NaiveTime, Offset, TimeZone, Timelike};

/// Resolve a one-off fire time to an epoch. ISO/RFC3339 and `YYYY-MM-DD HH:MM`
/// are honoured; bare `HH:MM` means today, or tomorrow if already past.
// ponytail: no full natural-language date parsing — the model normalizes
// "tomorrow at 3pm" into an ISO datetime before calling. This only covers
// the two shapes a human types directly.
pub(super) fn resolve_at(raw: &str) -> Result<f64, String> {
    let raw = raw.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        return Ok(dt.timestamp() as f64);
    }
    for fmt in [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M",
    ] {
        if let Ok(nd) = NaiveDateTime::parse_from_str(raw, fmt) {
            return local_epoch(nd);
        }
    }
    if let Ok(time) = NaiveTime::parse_from_str(raw, "%H:%M") {
        let now = Local::now();
        let mut nd = now.date_naive().and_time(time);
        if local_epoch(nd).is_ok_and(|e| e <= now.timestamp() as f64) {
            nd = (now.date_naive() + Duration::days(1)).and_time(time);
        }
        return local_epoch(nd);
    }
    Err(format!(
        "could not parse time '{raw}' — use \"HH:MM\" or an ISO datetime like 2026-07-20T09:00"
    ))
}

/// A local wall time to an epoch, honest about daylight saving: the
/// spring-forward gap is an error (that time never happens), the fall-back
/// fold takes the FIRST occurrence (the reminder fires sooner, not later).
fn local_epoch(nd: NaiveDateTime) -> Result<f64, String> {
    match Local.from_local_datetime(&nd) {
        chrono::LocalResult::Single(dt) => Ok(dt.timestamp() as f64),
        chrono::LocalResult::Ambiguous(first, _second) => Ok(first.timestamp() as f64),
        chrono::LocalResult::None => Err(format!(
            "{nd} does not exist in your timezone (skipped by daylight saving) — pick another time"
        )),
    }
}

/// regent-cron's `daily HH:MM` is UTC wall time; a user saying "daily 09:30"
/// means LOCAL. Convert with the current UTC offset so it fires at 9:30
/// where the user lives. Non-daily recurrences pass through untouched.
// ponytail: fixed-offset conversion — across a DST change the fire time
// shifts by the DST delta until the reminder is re-added; per-zone daily
// schedules belong in regent-cron if that ever matters.
pub(super) fn daily_to_utc(every: &str) -> String {
    let Some(hm) = every.trim().strip_prefix("daily ") else {
        return every.to_owned();
    };
    let Ok(t) = NaiveTime::parse_from_str(hm.trim(), "%H:%M") else {
        return every.to_owned(); // let Schedule::parse produce the error
    };
    let offset = i64::from(Local::now().offset().fix().local_minus_utc());
    let local_secs = i64::from(t.num_seconds_from_midnight());
    let utc = (local_secs - offset).rem_euclid(86_400);
    format!("daily {:02}:{:02}", utc / 3_600, (utc % 3_600) / 60)
}

pub(super) fn fmt_local(epoch: f64) -> String {
    Local
        .timestamp_opt(epoch as i64, 0)
        .single()
        .map(|dt| dt.format("%A, %B %e, %Y at %I:%M %p (UTC%:z)").to_string())
        .unwrap_or_else(|| format!("epoch {epoch}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_recurrence_converts_local_to_utc_and_back() {
        let converted = daily_to_utc("daily 09:30");
        let hm = converted.strip_prefix("daily ").unwrap();
        let t = NaiveTime::parse_from_str(hm, "%H:%M").unwrap();
        // Converting back with the same offset lands on 09:30 local again.
        let offset = i64::from(Local::now().offset().fix().local_minus_utc());
        let back = (i64::from(t.num_seconds_from_midnight()) + offset).rem_euclid(86_400);
        assert_eq!((back / 3_600, (back % 3_600) / 60), (9, 30), "{converted}");
    }

    #[test]
    fn non_daily_recurrences_pass_through() {
        assert_eq!(daily_to_utc("30m"), "30m");
        assert_eq!(daily_to_utc("2h"), "2h");
        assert_eq!(daily_to_utc("daily nonsense"), "daily nonsense");
    }

    #[test]
    fn resolve_at_still_handles_the_three_shapes() {
        assert!(resolve_at("2999-01-01T09:00:00").unwrap() > 0.0);
        assert!(resolve_at("12:34").is_ok());
        assert!(resolve_at("someday").unwrap_err().contains("could not parse"));
    }
}
