use super::*;
use crate::domain::contracts::DenyAll;
use std::sync::Arc;

fn ctx() -> ToolContext {
    ToolContext::new(std::path::PathBuf::from("."), Arc::new(DenyAll))
}

#[tokio::test]
async fn no_args_returns_the_default_zones() {
    let ctx = ctx();
    let out = WorldTimeTool.execute(json!({}), &ctx).await.unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    let zones = v["zones"].as_array().unwrap();
    assert_eq!(zones.len(), DEFAULT_ZONES.len(), "{v}");
    assert_eq!(zones[0]["zone"].as_str().unwrap(), "UTC", "{v}");
    assert_eq!(zones[0]["day"].as_str().unwrap(), "same day", "{v}");
}

#[tokio::test]
async fn converts_a_given_time_between_zones() {
    let ctx = ctx();
    let out = WorldTimeTool
        .execute(
            json!({"time": "12:00", "from_zone": "UTC", "zones": ["Asia/Tokyo"]}),
            &ctx,
        )
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    let tokyo = &v["zones"][0];
    // Tokyo is UTC+9 year-round (no DST), so noon UTC is 21:00 the same day.
    assert!(tokyo["time"].as_str().unwrap().ends_with("21:00:00"), "{v}");
    assert_eq!(tokyo["utc_offset"].as_str().unwrap(), "+09:00", "{v}");
}

#[tokio::test]
async fn a_late_night_conversion_can_roll_into_the_next_day() {
    let ctx = ctx();
    let out = WorldTimeTool
        .execute(
            json!({"time": "23:00", "from_zone": "UTC", "zones": ["Asia/Tokyo"]}),
            &ctx,
        )
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["zones"][0]["day"].as_str().unwrap(), "+1d", "{v}");
}

#[tokio::test]
async fn unknown_zone_suggests_close_matches() {
    let ctx = ctx();
    let out = WorldTimeTool
        .execute(json!({"zones": ["Europe/Londo"]}), &ctx)
        .await
        .unwrap();
    assert!(out.contains("unknown time zone"), "got: {out}");
    assert!(out.contains("Europe/London"), "got: {out}");
}

#[tokio::test]
async fn unparseable_time_is_a_clear_error() {
    let ctx = ctx();
    let out = WorldTimeTool
        .execute(json!({"time": "not-a-time"}), &ctx)
        .await
        .unwrap();
    assert!(out.contains("cannot parse time"), "got: {out}");
}
