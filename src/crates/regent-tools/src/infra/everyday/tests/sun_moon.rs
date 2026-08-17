//! Unit tests for `sun_moon` — URL construction, response parsing, and the
//! local moon-phase math. No network involved.

use super::*;

const SAMPLE_SUN: &str = r#"{
    "daily": {
        "time": ["2026-07-16"],
        "sunrise": ["2026-07-16T05:12"],
        "sunset": ["2026-07-16T20:47"],
        "daylight_duration": [56100.0]
    }
}"#;

const MISSING_DAILY: &str = r#"{"generationtime_ms": 0.1}"#;
const MISSING_TIME: &str = r#"{"daily": {"sunrise": ["x"]}}"#;

#[test]
fn url_uses_forecast_days_without_a_date() {
    let url = build_sun_url(51.5074, -0.1278, None);
    assert!(url.contains("latitude=51.5074"));
    assert!(url.contains("longitude=-0.1278"));
    assert!(url.contains("forecast_days=1"));
    assert!(url.contains("daily=sunrise") && url.contains("daylight_duration"));
    assert!(!url.contains("start_date"));
}

#[test]
fn url_uses_start_and_end_date_with_a_date() {
    let url = build_sun_url(51.5074, -0.1278, Some("2026-08-01"));
    assert!(url.contains("start_date=2026-08-01"));
    assert!(url.contains("end_date=2026-08-01"));
    assert!(!url.contains("forecast_days"));
}

#[test]
fn parses_sunrise_sunset_and_daylight_hours() {
    let v = parse_sun_response(SAMPLE_SUN.as_bytes()).unwrap();
    assert_eq!(v["date"], "2026-07-16");
    assert_eq!(v["sunrise"], "2026-07-16T05:12");
    assert_eq!(v["sunset"], "2026-07-16T20:47");
    assert_eq!(v["daylight_hours"], 15.58);
}

#[test]
fn missing_daily_is_a_clear_error() {
    let err = parse_sun_response(MISSING_DAILY.as_bytes()).unwrap_err();
    assert!(err.contains("missing daily"), "{err}");
}

#[test]
fn missing_time_is_a_clear_error() {
    let err = parse_sun_response(MISSING_TIME.as_bytes()).unwrap_err();
    assert!(err.contains("missing daily.time"), "{err}");
}

#[test]
fn malformed_json_is_a_clear_error() {
    let err = parse_sun_response(b"not json").unwrap_err();
    assert!(err.contains("bad sun response"), "{err}");
}

#[test]
fn moon_phase_is_new_at_the_reference_epoch() {
    let (name, illumination) = moon_phase_for(NaiveDate::from_ymd_opt(2000, 1, 6).unwrap());
    assert_eq!(name, "new moon");
    assert!(illumination < 5.0, "{illumination}");
}

#[test]
fn moon_phase_is_full_half_a_synodic_month_later() {
    let (name, illumination) = moon_phase_for(NaiveDate::from_ymd_opt(2000, 1, 21).unwrap());
    assert_eq!(name, "full moon");
    assert!(illumination > 95.0, "{illumination}");
}

#[tokio::test]
async fn invalid_date_is_a_clear_tool_error() {
    let ctx = ToolContext::new(
        std::path::PathBuf::from("."),
        std::sync::Arc::new(crate::domain::contracts::DenyAll),
    );
    let out = SunMoonTool
        .execute(json!({"place": "Oslo", "date": "not-a-date"}), &ctx)
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert!(v["error"].as_str().unwrap().contains("invalid date"), "{v}");
}
