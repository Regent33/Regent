use super::*;
use crate::domain::contracts::DenyAll;
use std::sync::Arc;

fn ctx() -> ToolContext {
    ToolContext::new(std::path::PathBuf::from("."), Arc::new(DenyAll))
}

#[tokio::test]
async fn days_between_two_dates() {
    let ctx = ctx();
    let out = DateCalcTool
        .execute(
            json!({"action": "days_between", "date": "2026-01-01", "date2": "2026-01-31"}),
            &ctx,
        )
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["days"].as_i64().unwrap(), 30, "{v}");
}

#[tokio::test]
async fn add_months_handles_month_end_overflow() {
    let ctx = ctx();
    let out = DateCalcTool
        .execute(
            json!({"action": "add", "date": "2026-01-31", "amount": 1, "unit": "months"}),
            &ctx,
        )
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    // chrono clamps Jan 31 + 1 month to Feb 28 (2026 is not a leap year).
    assert_eq!(v["result"].as_str().unwrap(), "2026-02-28", "{v}");
}

#[tokio::test]
async fn subtract_via_negative_amount() {
    let ctx = ctx();
    let out = DateCalcTool
        .execute(
            json!({"action": "add", "date": "2026-07-16", "amount": -10, "unit": "days"}),
            &ctx,
        )
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["result"].as_str().unwrap(), "2026-07-06", "{v}");
}

#[tokio::test]
async fn weekday_of_a_known_date() {
    let ctx = ctx();
    // 2026-07-16 is a Thursday.
    let out = DateCalcTool
        .execute(json!({"action": "weekday", "date": "2026-07-16"}), &ctx)
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["weekday"].as_str().unwrap(), "Thursday", "{v}");
}

#[tokio::test]
async fn age_from_a_birthdate_before_this_years_birthday() {
    let ctx = ctx();
    let today = Local::now().date_naive();
    // A birthdate later in the calendar year than today: hasn't happened yet
    // this year, so age is (this year - birth year - 1).
    let future_month_day = if today.month() == 12 {
        1
    } else {
        today.month() + 1
    };
    let birth = NaiveDate::from_ymd_opt(today.year() - 30, future_month_day, 1).unwrap();
    let out = DateCalcTool
        .execute(json!({"action": "age", "date": birth.to_string()}), &ctx)
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["age_years"].as_i64().unwrap(), 29, "{v}");
}

#[tokio::test]
async fn unparseable_date_is_a_clear_error() {
    let ctx = ctx();
    let out = DateCalcTool
        .execute(json!({"action": "weekday", "date": "16/07/2026"}), &ctx)
        .await
        .unwrap();
    assert!(out.contains("expected YYYY-MM-DD"), "got: {out}");
}

#[tokio::test]
async fn unknown_action_is_an_error() {
    let ctx = ctx();
    let out = DateCalcTool
        .execute(json!({"action": "moon_phase", "date": "2026-07-16"}), &ctx)
        .await
        .unwrap();
    assert!(out.contains("unknown action"), "got: {out}");
}
