//! Unit tests for the shared geocoding helper. Parsing is exercised against
//! canned JSON strings — no network involved.

use super::*;

const SAMPLE_RESULTS: &str = r#"{
    "results": [
        {"name": "Paris", "country": "France", "latitude": 48.8534, "longitude": 2.3488},
        {"name": "Paris", "country": "United States", "latitude": 33.6609, "longitude": -95.5555}
    ]
}"#;

const EMPTY_RESULTS: &str = r#"{"results": []}"#;
const NO_RESULTS_KEY: &str = r#"{"generationtime_ms": 0.1}"#;

#[test]
fn url_carries_the_query_params() {
    let url = geocode_url("San Francisco");
    assert!(url.starts_with("https://geocoding-api.open-meteo.com/v1/search?"));
    assert!(url.contains("name=San+Francisco") || url.contains("name=San%20Francisco"));
    assert!(url.contains("count=1"));
    assert!(url.contains("format=json"));
}

#[test]
fn parses_the_first_result() {
    let g = parse_geocode_response(SAMPLE_RESULTS.as_bytes(), "paris").unwrap();
    assert_eq!(g.name, "Paris");
    assert_eq!(g.country, "France");
    assert!((g.latitude - 48.8534).abs() < 1e-6);
    assert!((g.longitude - 2.3488).abs() < 1e-6);
}

#[test]
fn empty_results_is_a_clear_error() {
    let err = parse_geocode_response(EMPTY_RESULTS.as_bytes(), "nowhereville").unwrap_err();
    assert!(err.contains("no location found for 'nowhereville'"), "{err}");
}

#[test]
fn missing_results_key_is_a_clear_error() {
    let err = parse_geocode_response(NO_RESULTS_KEY.as_bytes(), "atlantis").unwrap_err();
    assert!(err.contains("no location found for 'atlantis'"), "{err}");
}

#[test]
fn malformed_json_is_a_clear_error() {
    let err = parse_geocode_response(b"not json", "x").unwrap_err();
    assert!(err.contains("bad geocoding response"), "{err}");
}

#[tokio::test]
async fn resolve_location_prefers_explicit_coordinates() {
    let args = serde_json::json!({"latitude": 10.5, "longitude": 20.5, "place": "should be ignored"});
    let (lat, lon, resolved) = resolve_location(&args).await.unwrap();
    assert_eq!(lat, 10.5);
    assert_eq!(lon, 20.5);
    assert!(resolved.is_none());
}

#[tokio::test]
async fn resolve_location_requires_place_or_coordinates() {
    let args = serde_json::json!({});
    let err = resolve_location(&args).await.unwrap_err();
    assert!(err.contains("provide either 'place'"), "{err}");
}
