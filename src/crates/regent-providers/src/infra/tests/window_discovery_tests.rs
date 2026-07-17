//! Unit tests for `window_discovery` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

#[test]
fn parses_openrouter_catalog_shape() {
    let body = serde_json::json!({
        "data": [
            {"id": "anthropic/claude-fable-5", "context_length": 1_000_000, "pricing": {}},
            {"id": "openai/gpt-5", "context_length": 272_000},
            {"id": "no-window-field"},
        ]
    })
    .to_string();
    let parsed = parse_openrouter_catalog(&body);
    assert_eq!(
        parsed,
        vec![
            ("anthropic/claude-fable-5".to_owned(), 1_000_000),
            ("openai/gpt-5".to_owned(), 272_000),
        ]
    );
}

#[test]
fn malformed_openrouter_body_yields_no_entries() {
    assert!(parse_openrouter_catalog("not json").is_empty());
    assert!(parse_openrouter_catalog(r#"{"unexpected": true}"#).is_empty());
}

#[test]
fn parses_anthropic_model_shape() {
    let body = serde_json::json!({
        "id": "claude-fable-5",
        "type": "model",
        "max_input_tokens": 1_000_000,
    })
    .to_string();
    assert_eq!(parse_anthropic_model(&body), Some(1_000_000));
}

#[test]
fn anthropic_body_missing_the_field_yields_none() {
    assert_eq!(parse_anthropic_model(r#"{"id": "x"}"#), None);
    assert_eq!(parse_anthropic_model("not json"), None);
}

#[test]
fn cache_hit_on_second_call_without_a_refetch() {
    // Distinct endpoint key so this doesn't collide with other tests sharing
    // the process-wide cache.
    let endpoint = "test-endpoint-cache-behavior";
    assert_eq!(discovered_window(endpoint, "some-model"), None);
    store(endpoint, "some-model", 555_000);
    // Both calls read the same cached entry — no fetch happens here at all,
    // which is the point: a discovered window is never re-fetched mid-process.
    assert_eq!(discovered_window(endpoint, "some-model"), Some(555_000));
    assert_eq!(discovered_window(endpoint, "some-model"), Some(555_000));
}

#[test]
fn cache_is_keyed_by_endpoint_and_model_together() {
    store("endpoint-a", "shared-model-name", 100_000);
    store("endpoint-b", "shared-model-name", 200_000);
    assert_eq!(
        discovered_window("endpoint-a", "shared-model-name"),
        Some(100_000)
    );
    assert_eq!(
        discovered_window("endpoint-b", "shared-model-name"),
        Some(200_000)
    );
}

#[tokio::test]
async fn spawn_functions_are_a_noop_outside_a_runtime_or_never_panic() {
    // Both spawn helpers must never panic even when pointed at a bogus host —
    // discovery failure is silent by design. This runs inside a runtime (so
    // the fetch is actually spawned) purely to prove it doesn't bring down
    // the process; the network call itself will fail fast and is ignored.
    spawn_openrouter_discovery("http://127.0.0.1:0".to_owned());
    spawn_anthropic_discovery(
        Client::new(),
        "http://127.0.0.1:0".to_owned(),
        "key".to_owned(),
        "2023-06-01".to_owned(),
        "model".to_owned(),
    );
}
