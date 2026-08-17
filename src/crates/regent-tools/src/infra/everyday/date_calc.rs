//! `date_calc` — calendar arithmetic offline via `chrono`: days between two
//! dates, add/subtract days-weeks-months, days until a date, age from a
//! birthdate, and the weekday of a date. Dates are `YYYY-MM-DD`.

use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use chrono::{Datelike, Duration, Local, Months, NaiveDate};
use regent_kernel::{RegentError, ToolDefinition, tool_error_json, tool_result_json};
use serde_json::{Value, json};

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "date_calc".into(),
        description: "Calendar arithmetic on YYYY-MM-DD dates. `action`: \"days_between\" \
                      (date, date2), \"add\" (date, amount, unit: days|weeks|months — negative \
                      amount subtracts), \"days_until\" (date), \"age\" (date = birthdate), or \
                      \"weekday\" (date)."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["days_between", "add", "days_until", "age", "weekday"]
                },
                "date": {"type": "string", "description": "YYYY-MM-DD"},
                "date2": {"type": "string", "description": "YYYY-MM-DD (days_between only)"},
                "amount": {"type": "integer", "description": "Signed offset (add only)."},
                "unit": {
                    "type": "string",
                    "enum": ["days", "weeks", "months"],
                    "description": "Unit for `amount` (add only, default days)."
                }
            },
            "required": ["action"]
        }),
        toolset: "everyday".into(),
    }
}

pub struct DateCalcTool;

#[async_trait]
impl ToolExecutor for DateCalcTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(action) = args.get("action").and_then(Value::as_str) else {
            return Ok(tool_error_json("missing required parameter: action"));
        };

        match action {
            "days_between" => days_between(&args),
            "add" => add(&args),
            "days_until" => days_until(&args),
            "age" => age(&args),
            "weekday" => weekday(&args),
            other => Ok(tool_error_json(format!(
                "unknown action '{other}' — use days_between, add, days_until, age, or weekday"
            ))),
        }
    }
}

fn parse_date(raw: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .map_err(|_| format!("cannot parse date '{raw}' — expected YYYY-MM-DD"))
}

fn required_date(args: &Value, field: &str) -> Result<NaiveDate, String> {
    let Some(raw) = args.get(field).and_then(Value::as_str) else {
        return Err(format!("missing required parameter: {field}"));
    };
    parse_date(raw)
}

fn days_between(args: &Value) -> Result<String, RegentError> {
    let date = match required_date(args, "date") {
        Ok(d) => d,
        Err(e) => return Ok(tool_error_json(e)),
    };
    let date2 = match required_date(args, "date2") {
        Ok(d) => d,
        Err(e) => return Ok(tool_error_json(e)),
    };
    let days = date2.signed_duration_since(date).num_days();
    Ok(tool_result_json(json!({
        "date": date.to_string(),
        "date2": date2.to_string(),
        "days": days,
        "absolute_days": days.abs(),
    })))
}

fn add(args: &Value) -> Result<String, RegentError> {
    let date = match required_date(args, "date") {
        Ok(d) => d,
        Err(e) => return Ok(tool_error_json(e)),
    };
    let Some(amount) = args.get("amount").and_then(Value::as_i64) else {
        return Ok(tool_error_json("missing required parameter: amount"));
    };
    let unit = args.get("unit").and_then(Value::as_str).unwrap_or("days");
    let result = match unit {
        "days" => date.checked_add_signed(Duration::days(amount)),
        "weeks" => date.checked_add_signed(Duration::weeks(amount)),
        "months" => {
            if amount >= 0 {
                date.checked_add_months(Months::new(amount as u32))
            } else {
                date.checked_sub_months(Months::new(amount.unsigned_abs() as u32))
            }
        }
        other => {
            return Ok(tool_error_json(format!(
                "unknown unit '{other}' — use days, weeks, or months"
            )));
        }
    };
    let Some(result) = result else {
        return Ok(tool_error_json(
            "that date arithmetic overflowed the valid date range",
        ));
    };
    let mut out = json!({
        "date": date.to_string(),
        "amount": amount,
        "unit": unit,
        "result": result.to_string(),
    });
    // Month arithmetic clamps to the target month's last day (Jan 31 + 1
    // month = Feb 28) — say so instead of silently drifting the day.
    if unit == "months" && result.day() != date.day() {
        out["note"] = json!(format!(
            "day clamped to the end of the month ({} has no day {})",
            result.format("%B %Y"),
            date.day()
        ));
    }
    Ok(tool_result_json(out))
}

fn days_until(args: &Value) -> Result<String, RegentError> {
    let date = match required_date(args, "date") {
        Ok(d) => d,
        Err(e) => return Ok(tool_error_json(e)),
    };
    let today = Local::now().date_naive();
    let days = date.signed_duration_since(today).num_days();
    let status = match days.cmp(&0) {
        std::cmp::Ordering::Equal => "today",
        std::cmp::Ordering::Greater => "future",
        std::cmp::Ordering::Less => "past",
    };
    Ok(tool_result_json(json!({
        "date": date.to_string(),
        "today": today.to_string(),
        "days_until": days,
        "status": status,
    })))
}

fn age(args: &Value) -> Result<String, RegentError> {
    let birth = match required_date(args, "date") {
        Ok(d) => d,
        Err(e) => return Ok(tool_error_json(e)),
    };
    let today = Local::now().date_naive();
    if birth > today {
        return Ok(tool_error_json(format!(
            "'{birth}' is in the future — a birthdate can't be after today ({today})"
        )));
    }
    let mut years = today.year() - birth.year();
    let had_birthday_this_year = (today.month(), today.day()) >= (birth.month(), birth.day());
    if !had_birthday_this_year {
        years -= 1;
    }
    Ok(tool_result_json(json!({
        "birthdate": birth.to_string(),
        "as_of": today.to_string(),
        "age_years": years,
    })))
}

fn weekday(args: &Value) -> Result<String, RegentError> {
    let date = match required_date(args, "date") {
        Ok(d) => d,
        Err(e) => return Ok(tool_error_json(e)),
    };
    Ok(tool_result_json(json!({
        "date": date.to_string(),
        "weekday": date.format("%A").to_string(),
    })))
}

#[cfg(test)]
#[path = "tests/date_calc.rs"]
mod tests;
