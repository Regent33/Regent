//! Unit tests for `weather` — URL construction and response parsing against
//! canned JSON. No network involved.

use super::*;

const SAMPLE_FORECAST: &str = r#"{
    "current_weather": {"temperature": 18.4, "windspeed": 12.1, "weathercode": 2, "time": "2026-07-16T09:00"},
    "daily": {
        "time": ["2026-07-16", "2026-07-17"],
        "temperature_2m_max": [22.5, 24.1],
        "temperature_2m_min": [14.2, 15.0],
        "precipitation_probability_max": [10, 40],
        "weathercode": [2, 61]
    }
}"#;

const MISSING_CURRENT: &str = r#"{"daily": {"time": []}}"#;
const MISSING_DAILY: &str = r#"{"current_weather": {"weathercode": 0}}"#;

#[test]
fn url_carries_coordinates_and_days() {
    let url = build_forecast_url(35.6762, 139.6503, 3);
    assert!(url.starts_with("https://api.open-meteo.com/v1/forecast?"));
    assert!(url.contains("latitude=35.6762"));
    assert!(url.contains("longitude=139.6503"));
    assert!(url.contains("forecast_days=3"));
    assert!(url.contains("current_weather=true"));
    assert!(url.contains("timezone=auto"));
}

#[test]
fn parses_current_and_daily() {
    let v = parse_forecast_response(SAMPLE_FORECAST.as_bytes()).unwrap();
    assert_eq!(v["current"]["temperature_c"], 18.4);
    assert_eq!(v["current"]["conditions"], "partly cloudy");
    let daily = v["daily"].as_array().unwrap();
    assert_eq!(daily.len(), 2);
    assert_eq!(daily[0]["date"], "2026-07-16");
    assert_eq!(daily[0]["temp_max_c"], 22.5);
    assert_eq!(daily[1]["conditions"], "rain");
    assert_eq!(daily[1]["precipitation_probability_max"], 40);
}

#[test]
fn missing_current_weather_is_a_clear_error() {
    let err = parse_forecast_response(MISSING_CURRENT.as_bytes()).unwrap_err();
    assert!(err.contains("missing current_weather"), "{err}");
}

#[test]
fn missing_daily_is_a_clear_error() {
    let err = parse_forecast_response(MISSING_DAILY.as_bytes()).unwrap_err();
    assert!(err.contains("missing daily"), "{err}");
}

#[test]
fn malformed_json_is_a_clear_error() {
    let err = parse_forecast_response(b"not json").unwrap_err();
    assert!(err.contains("bad forecast response"), "{err}");
}

#[test]
fn weathercodes_map_to_readable_strings() {
    assert_eq!(weathercode_description(0), "clear sky");
    assert_eq!(weathercode_description(63), "rain");
    assert_eq!(weathercode_description(95), "thunderstorm");
    assert_eq!(weathercode_description(12345), "unknown conditions");
}

#[tokio::test]
async fn missing_location_is_a_clear_tool_error() {
    let ctx = ToolContext::new(
        std::path::PathBuf::from("."),
        std::sync::Arc::new(crate::domain::contracts::DenyAll),
    );
    let out = WeatherTool.execute(json!({}), &ctx).await.unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert!(v["error"].as_str().unwrap().contains("weather:"), "{v}");
}
