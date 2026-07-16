//! `world_time` — convert times across IANA time zones, fully offline via
//! `chrono-tz`'s embedded tzdata. No args -> now, in a handful of major
//! zones. With `time` (+ optional `from_zone`) -> that moment converted into
//! every requested zone, with UTC offset and a same-day/+1d/-1d marker
//! relative to the reference zone's calendar day.

use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::{TZ_VARIANTS, Tz};
use regent_kernel::{RegentError, ToolDefinition, tool_error_json, tool_result_json};
use serde_json::{Value, json};
use std::str::FromStr;

/// Shown when the caller asks for "the time" without naming zones.
const DEFAULT_ZONES: &[&str] = &[
    "UTC",
    "America/New_York",
    "America/Los_Angeles",
    "Europe/London",
    "Asia/Tokyo",
    "Asia/Kolkata",
    "Australia/Sydney",
];

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "world_time".into(),
        description: "The current time in one or more IANA time zones, or convert a specific \
                      time between zones. No args returns a handful of major zones right now. \
                      Pass `time` (\"HH:MM\" or an ISO date-time) with an optional `from_zone` \
                      (default UTC) to convert that moment into each requested `zones` entry."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "zones": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "IANA zone names, e.g. [\"Europe/Paris\", \"Asia/Tokyo\"]."
                },
                "time": {
                    "type": "string",
                    "description": "\"HH:MM\" or an ISO date-time. Omit for the current moment."
                },
                "from_zone": {
                    "type": "string",
                    "description": "IANA zone `time` is expressed in (default UTC)."
                }
            }
        }),
        toolset: "everyday".into(),
    }
}

pub struct WorldTimeTool;

#[async_trait]
impl ToolExecutor for WorldTimeTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let zone_names: Vec<String> = args
            .get("zones")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .filter(|v: &Vec<String>| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_ZONES.iter().map(|s| (*s).to_string()).collect());

        let mut zones = Vec::with_capacity(zone_names.len());
        for name in &zone_names {
            match parse_zone(name) {
                Ok(tz) => zones.push((name.clone(), tz)),
                Err(message) => return Ok(tool_error_json(message)),
            }
        }

        let from_zone_name = args
            .get("from_zone")
            .and_then(Value::as_str)
            .unwrap_or("UTC");
        let from_tz = match parse_zone(from_zone_name) {
            Ok(tz) => tz,
            Err(message) => return Ok(tool_error_json(message)),
        };

        let reference: DateTime<Tz> = match args.get("time").and_then(Value::as_str) {
            Some(time) => match parse_time_in_zone(time, from_tz) {
                Ok(dt) => dt,
                Err(message) => return Ok(tool_error_json(message)),
            },
            None => Utc::now().with_timezone(&from_tz),
        };
        let reference_date = reference.date_naive();

        let results: Vec<Value> = zones
            .iter()
            .map(|(name, tz)| {
                let converted = reference.with_timezone(tz);
                let diff = (converted.date_naive() - reference_date).num_days();
                let day = if diff == 0 {
                    "same day".to_string()
                } else {
                    format!("{diff:+}d")
                };
                json!({
                    "zone": name,
                    "time": converted.format("%Y-%m-%d %H:%M:%S").to_string(),
                    "abbreviation": converted.format("%Z").to_string(),
                    "utc_offset": converted.format("%:z").to_string(),
                    "day": day,
                })
            })
            .collect();

        Ok(tool_result_json(json!({
            "reference_utc": reference.with_timezone(&Utc).to_rfc3339(),
            "from_zone": from_tz.name(),
            "zones": results,
        })))
    }
}

/// Resolves an IANA name, or (on failure) a helpful error naming close
/// substring matches from the full `TZ_VARIANTS` table.
fn parse_zone(name: &str) -> Result<Tz, String> {
    Tz::from_str(name).map_err(|_| {
        let needle = name.to_lowercase();
        let mut matches: Vec<&str> = TZ_VARIANTS
            .iter()
            .map(|tz| tz.name())
            .filter(|candidate| candidate.to_lowercase().contains(&needle))
            .take(5)
            .collect();
        matches.sort_unstable();
        if matches.is_empty() {
            format!(
                "unknown time zone '{name}' — use an IANA name like 'Europe/London' or \
                 'Asia/Tokyo'"
            )
        } else {
            format!(
                "unknown time zone '{name}' — did you mean: {}?",
                matches.join(", ")
            )
        }
    })
}

/// Parses `time` as an ISO date-time (with its own offset), a naive
/// date-time, or a bare "HH:MM"/"HH:MM:SS" applied to today's date in `tz`.
fn parse_time_in_zone(time: &str, tz: Tz) -> Result<DateTime<Tz>, String> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(time) {
        return Ok(dt.with_timezone(&tz));
    }
    for fmt in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(time, fmt) {
            return localize(naive, tz, time);
        }
    }
    for fmt in ["%H:%M:%S", "%H:%M"] {
        if let Ok(t) = NaiveTime::parse_from_str(time, fmt) {
            let today = Utc::now().with_timezone(&tz).date_naive();
            return localize(today.and_time(t), tz, time);
        }
    }
    Err(format!(
        "cannot parse time '{time}' — use \"HH:MM\", or an ISO date-time like \
         2026-07-16T14:30"
    ))
}

fn localize(naive: NaiveDateTime, tz: Tz, original: &str) -> Result<DateTime<Tz>, String> {
    tz.from_local_datetime(&naive).single().ok_or_else(|| {
        format!("'{original}' falls in a DST gap or is ambiguous in {tz} — pick another time")
    })
}

#[cfg(test)]
#[path = "world_time_tests.rs"]
mod tests;
