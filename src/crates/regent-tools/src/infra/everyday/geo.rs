//! Shared Open-Meteo geocoding helper for `weather` and `sun_moon` — resolves
//! a free-text place name to coordinates + a display name/country, or passes
//! explicit `latitude`/`longitude` args straight through. Kept in one file so
//! both tools share a single implementation instead of diverging.

use super::http;
use serde_json::Value;

const GEOCODE_URL: &str = "https://geocoding-api.open-meteo.com/v1/search";

/// One resolved place: display name, country, and coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct GeoResult {
    pub name: String,
    pub country: String,
    pub latitude: f64,
    pub longitude: f64,
}

/// Build the Open-Meteo geocoding request URL for `place` (pure — no I/O).
pub fn geocode_url(place: &str) -> String {
    let mut url = reqwest::Url::parse(GEOCODE_URL).expect("static url");
    url.query_pairs_mut()
        .append_pair("name", place)
        .append_pair("count", "1")
        .append_pair("format", "json");
    url.to_string()
}

/// Parse an Open-Meteo geocoding response body into the first (best) match.
pub fn parse_geocode_response(body: &[u8], place: &str) -> Result<GeoResult, String> {
    let v: Value =
        serde_json::from_slice(body).map_err(|e| format!("bad geocoding response: {e}"))?;
    let first = v
        .get("results")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .ok_or_else(|| format!("no location found for '{place}'"))?;
    Ok(GeoResult {
        name: first
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(place)
            .to_owned(),
        country: first
            .get("country")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        latitude: first
            .get("latitude")
            .and_then(Value::as_f64)
            .ok_or("geocoding response missing latitude")?,
        longitude: first
            .get("longitude")
            .and_then(Value::as_f64)
            .ok_or("geocoding response missing longitude")?,
    })
}

/// Geocode `place` via the Open-Meteo geocoding API (network I/O; keyless).
pub async fn geocode(place: &str) -> Result<GeoResult, String> {
    let bytes = http::fetch_ok("open-meteo geocoding", &geocode_url(place)).await?;
    parse_geocode_response(&bytes, place)
}

/// Resolve a tool's location args to `(latitude, longitude, resolved_place)`.
/// Explicit `latitude`/`longitude` win outright (no network); otherwise
/// `place` is geocoded. `resolved_place` is `Some((name, country))` only when
/// geocoding ran.
pub async fn resolve_location(
    args: &Value,
) -> Result<(f64, f64, Option<(String, String)>), String> {
    let lat = args.get("latitude").and_then(Value::as_f64);
    let lon = args.get("longitude").and_then(Value::as_f64);
    if let (Some(lat), Some(lon)) = (lat, lon) {
        return Ok((lat, lon, None));
    }
    let place = args
        .get("place")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or("provide either 'place' or both 'latitude' and 'longitude'")?;
    let g = geocode(place).await?;
    Ok((g.latitude, g.longitude, Some((g.name, g.country))))
}

#[cfg(test)]
#[path = "geo_tests.rs"]
mod tests;
