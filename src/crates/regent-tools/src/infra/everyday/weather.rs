//! `weather` — current conditions + a multi-day forecast from Open-Meteo (no
//! API key). Accepts a free-text `place` (geocoded via `geo::resolve_location`)
//! or explicit `latitude`/`longitude`. Network + JSON parsing are split so the
//! parsing half is unit-testable against canned response bodies.

use super::{geo, http};
use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json, tool_result_json};
use serde_json::{Value, json};

const FORECAST_URL: &str = "https://api.open-meteo.com/v1/forecast";

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "weather".into(),
        description: "Current weather conditions plus a daily forecast (temperature, \
                      precipitation chance, conditions). Give a place name, or explicit \
                      latitude/longitude. No API key needed."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "place": {"type": "string", "description": "A city or place name, e.g. 'Tokyo'."},
                "latitude": {"type": "number", "description": "Latitude, if not using 'place'."},
                "longitude": {"type": "number", "description": "Longitude, if not using 'place'."},
                "days": {"type": "integer", "description": "Forecast length, 1-7 (default 1 = today only)."}
            }
        }),
        toolset: "everyday".into(),
    }
}

pub struct WeatherTool;

#[async_trait]
impl ToolExecutor for WeatherTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let days = args
            .get("days")
            .and_then(Value::as_u64)
            .map_or(1, |n| n.clamp(1, 7));

        let (lat, lon, place) = match geo::resolve_location(&args).await {
            Ok(loc) => loc,
            Err(e) => return Ok(tool_error_json(format!("weather: {e}"))),
        };

        let url = build_forecast_url(lat, lon, days);
        match fetch_forecast(&url).await {
            Ok(mut data) => {
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
                }
                Ok(tool_result_json(data))
            }
            Err(e) => Ok(tool_error_json(format!("weather: {e}"))),
        }
    }
}

/// Build the Open-Meteo forecast request URL (pure — no I/O).
pub fn build_forecast_url(lat: f64, lon: f64, days: u64) -> String {
    let mut url = reqwest::Url::parse(FORECAST_URL).expect("static url");
    url.query_pairs_mut()
        .append_pair("latitude", &lat.to_string())
        .append_pair("longitude", &lon.to_string())
        .append_pair("current_weather", "true")
        .append_pair(
            "daily",
            "temperature_2m_max,temperature_2m_min,precipitation_probability_max,weathercode",
        )
        .append_pair("forecast_days", &days.to_string())
        .append_pair("timezone", "auto");
    url.to_string()
}

async fn fetch_forecast(url: &str) -> Result<Value, String> {
    let bytes = http::fetch_ok("open-meteo forecast", url).await?;
    parse_forecast_response(&bytes)
}

/// Parse an Open-Meteo forecast response into `{current, daily}` (pure).
pub fn parse_forecast_response(body: &[u8]) -> Result<Value, String> {
    let v: Value =
        serde_json::from_slice(body).map_err(|e| format!("bad forecast response: {e}"))?;
    let cw = v
        .get("current_weather")
        .ok_or("forecast response missing current_weather")?;
    let current_code = cw
        .get("weathercode")
        .and_then(Value::as_i64)
        .ok_or("forecast response missing current weathercode")?;

    let daily = v.get("daily").ok_or("forecast response missing daily")?;
    let dates = daily
        .get("time")
        .and_then(Value::as_array)
        .ok_or("forecast response missing daily.time")?;
    let tmax = daily.get("temperature_2m_max").and_then(Value::as_array);
    let tmin = daily.get("temperature_2m_min").and_then(Value::as_array);
    let pop = daily
        .get("precipitation_probability_max")
        .and_then(Value::as_array);
    let codes = daily.get("weathercode").and_then(Value::as_array);

    let days_out: Vec<Value> = dates
        .iter()
        .enumerate()
        .map(|(i, date)| {
            let code = codes
                .and_then(|c| c.get(i))
                .and_then(Value::as_i64)
                .unwrap_or(current_code);
            json!({
                "date": date.as_str().unwrap_or(""),
                "temp_max_c": tmax.and_then(|a| a.get(i)).and_then(Value::as_f64),
                "temp_min_c": tmin.and_then(|a| a.get(i)).and_then(Value::as_f64),
                "precipitation_probability_max": pop.and_then(|a| a.get(i)).and_then(Value::as_i64),
                "weathercode": code,
                "conditions": weathercode_description(code),
            })
        })
        .collect();

    Ok(json!({
        "current": {
            "temperature_c": cw.get("temperature").and_then(Value::as_f64),
            "windspeed_kmh": cw.get("windspeed").and_then(Value::as_f64),
            "weathercode": current_code,
            "conditions": weathercode_description(current_code),
        },
        "daily": days_out,
    }))
}

/// Map a WMO weather code to a short human string.
#[must_use]
pub fn weathercode_description(code: i64) -> &'static str {
    match code {
        0 => "clear sky",
        1 => "mainly clear",
        2 => "partly cloudy",
        3 => "overcast",
        45 | 48 => "fog",
        51 | 53 | 55 => "drizzle",
        56 | 57 => "freezing drizzle",
        61 | 63 | 65 => "rain",
        66 | 67 => "freezing rain",
        71 | 73 | 75 => "snow fall",
        77 => "snow grains",
        80..=82 => "rain showers",
        85 | 86 => "snow showers",
        95 => "thunderstorm",
        96 | 99 => "thunderstorm with hail",
        _ => "unknown conditions",
    }
}

#[cfg(test)]
#[path = "weather_tests.rs"]
mod tests;
