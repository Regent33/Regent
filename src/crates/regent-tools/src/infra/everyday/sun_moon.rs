//! `sun_moon` — sunrise/sunset/daylight for a place + date (Open-Meteo daily,
//! no API key), plus a locally-computed moon phase (no network for the moon:
//! a standard synodic-month approximation from a known new-moon epoch).

use super::{geo, http};
use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use chrono::{NaiveDate, TimeZone, Utc};
use regent_kernel::{RegentError, ToolDefinition, tool_error_json, tool_result_json};
use serde_json::{Value, json};

const SUN_URL: &str = "https://api.open-meteo.com/v1/forecast";
/// Mean length of a synodic (new-moon-to-new-moon) month, in days.
const SYNODIC_MONTH_DAYS: f64 = 29.53058867;

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "sun_moon".into(),
        description: "Sunrise, sunset, daylight length, and the moon phase for a place and date. \
                      Give a place name, or explicit latitude/longitude; date defaults to today \
                      (YYYY-MM-DD). No API key needed."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "place": {"type": "string", "description": "A city or place name, e.g. 'Reykjavik'."},
                "latitude": {"type": "number", "description": "Latitude, if not using 'place'."},
                "longitude": {"type": "number", "description": "Longitude, if not using 'place'."},
                "date": {"type": "string", "description": "Date as YYYY-MM-DD (default: today)."}
            }
        }),
        toolset: "everyday".into(),
    }
}

pub struct SunMoonTool;

#[async_trait]
impl ToolExecutor for SunMoonTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let date_str = args.get("date").and_then(Value::as_str).map(str::to_owned);
        let date_naive = match &date_str {
            Some(d) => match NaiveDate::parse_from_str(d, "%Y-%m-%d") {
                Ok(nd) => Some(nd),
                Err(_) => {
                    return Ok(tool_error_json(format!(
                        "sun_moon: invalid date '{d}', expected YYYY-MM-DD"
                    )));
                }
            },
            None => None,
        };

        let (lat, lon, place) = match geo::resolve_location(&args).await {
            Ok(loc) => loc,
            Err(e) => return Ok(tool_error_json(format!("sun_moon: {e}"))),
        };

        let url = build_sun_url(lat, lon, date_str.as_deref());
        let mut data = match fetch_sun(&url).await {
            Ok(v) => v,
            Err(e) => return Ok(tool_error_json(format!("sun_moon: {e}"))),
        };

        let moon_date = date_naive.unwrap_or_else(|| Utc::now().date_naive());
        let (phase_name, illumination_pct) = moon_phase_for(moon_date);

        if let Value::Object(ref mut map) = data {
            map.insert(
                "location".into(),
                json!({
                    "latitude": lat,
                    "longitude": lon,
                    "name": place.as_ref().map(|(n, _)| n.clone()),
                    "country": place.as_ref().map(|(_, c)| c.clone()),
                }),
            );
            map.insert("moon_phase".into(), json!(phase_name));
            map.insert(
                "moon_illumination_percent".into(),
                json!((illumination_pct * 10.0).round() / 10.0),
            );
        }
        Ok(tool_result_json(data))
    }
}

/// Build the Open-Meteo sunrise/sunset request URL (pure — no I/O). Without a
/// `date`, `forecast_days=1` returns today.
pub fn build_sun_url(lat: f64, lon: f64, date: Option<&str>) -> String {
    let mut url = reqwest::Url::parse(SUN_URL).expect("static url");
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("latitude", &lat.to_string())
            .append_pair("longitude", &lon.to_string())
            .append_pair("daily", "sunrise,sunset,daylight_duration")
            .append_pair("timezone", "auto");
        match date {
            Some(d) => {
                qp.append_pair("start_date", d).append_pair("end_date", d);
            }
            None => {
                qp.append_pair("forecast_days", "1");
            }
        }
    }
    url.to_string()
}

async fn fetch_sun(url: &str) -> Result<Value, String> {
    let bytes = http::fetch_ok("open-meteo sun", url).await?;
    parse_sun_response(&bytes)
}

/// Parse an Open-Meteo daily-sun response into `{date, sunrise, sunset,
/// daylight_hours}` (pure).
pub fn parse_sun_response(body: &[u8]) -> Result<Value, String> {
    let v: Value = serde_json::from_slice(body).map_err(|e| format!("bad sun response: {e}"))?;
    let daily = v.get("daily").ok_or("sun response missing daily")?;
    let dates = daily
        .get("time")
        .and_then(Value::as_array)
        .ok_or("sun response missing daily.time")?;
    let date = dates
        .first()
        .and_then(Value::as_str)
        .ok_or("sun response has no date")?;
    let sunrise = daily
        .get("sunrise")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        .unwrap_or("");
    let sunset = daily
        .get("sunset")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        .unwrap_or("");
    let daylight_secs = daily
        .get("daylight_duration")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_f64);
    Ok(json!({
        "date": date,
        "sunrise": sunrise,
        "sunset": sunset,
        "daylight_hours": daylight_secs.map(|s| (s / 3600.0 * 100.0).round() / 100.0),
    }))
}

/// Moon phase for `date`, computed locally from a known new-moon epoch and
/// the mean synodic month (~1-day accuracy — fine for "what phase is the
/// moon in"). Returns (phase name, illumination percent).
#[must_use]
pub fn moon_phase_for(date: NaiveDate) -> (&'static str, f64) {
    let reference_new_moon = Utc.with_ymd_and_hms(2000, 1, 6, 18, 14, 0).unwrap();
    let target = date.and_hms_opt(12, 0, 0).unwrap().and_utc();
    let days_since = (target - reference_new_moon).num_seconds() as f64 / 86400.0;
    let phase = days_since.rem_euclid(SYNODIC_MONTH_DAYS) / SYNODIC_MONTH_DAYS;
    let illumination_pct = (1.0 - (phase * 2.0 * std::f64::consts::PI).cos()) / 2.0 * 100.0;
    let name = match phase {
        p if !(0.03..0.97).contains(&p) => "new moon",
        p if p < 0.22 => "waxing crescent",
        p if p < 0.28 => "first quarter",
        p if p < 0.47 => "waxing gibbous",
        p if p < 0.53 => "full moon",
        p if p < 0.72 => "waning gibbous",
        p if p < 0.78 => "last quarter",
        _ => "waning crescent",
    };
    (name, illumination_pct)
}

#[cfg(test)]
#[path = "tests/sun_moon.rs"]
mod tests;
