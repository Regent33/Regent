//! Offline unit paths run through the executor; the currency path's pure
//! halves (URL construction, response parsing) are tested against canned
//! JSON — no network in unit tests.

use super::*;
use crate::domain::contracts::DenyAll;
use std::sync::Arc;

fn ctx() -> ToolContext {
    ToolContext::new(std::path::PathBuf::from("."), Arc::new(DenyAll))
}

async fn run(args: Value) -> Result<Value, RegentError> {
    ConvertTool
        .execute(args, &ctx())
        .await
        .map(|out| serde_json::from_str(&out).unwrap())
}

#[tokio::test]
async fn converts_length_mass_and_data() {
    let v = run(json!({"value": 5.0, "from": "km", "to": "mi"})).await.unwrap();
    assert!((v["result"].as_f64().unwrap() - 3.106_856).abs() < 1e-5, "{v}");

    let v = run(json!({"value": 150.0, "from": "lb", "to": "kg"})).await.unwrap();
    assert!((v["result"].as_f64().unwrap() - 68.038_855).abs() < 1e-5, "{v}");

    let v = run(json!({"value": 2.0, "from": "GB", "to": "mb"})).await.unwrap();
    assert_eq!(v["result"], 2000.0, "case-insensitive units: {v}");
}

#[tokio::test]
async fn temperature_is_affine_not_linear() {
    let v = run(json!({"value": 100.0, "from": "C", "to": "F"})).await.unwrap();
    assert_eq!(v["result"], 212.0, "{v}");
    let v = run(json!({"value": 0.0, "from": "c", "to": "k"})).await.unwrap();
    assert_eq!(v["result"], 273.15, "{v}");
}

#[tokio::test]
async fn cross_category_and_unknown_units_error_clearly() {
    let e = run(json!({"value": 1.0, "from": "kg", "to": "km"}))
        .await
        .unwrap_err()
        .to_string();
    assert!(e.contains("can't convert mass"), "{e}");

    let e = run(json!({"value": 1.0, "from": "wibbles", "to": "wobbles"}))
        .await
        .unwrap_err()
        .to_string();
    assert!(e.contains("unknown units"), "{e}");

    let e = run(json!({"from": "km", "to": "mi"})).await.unwrap_err().to_string();
    assert!(e.contains("`value`"), "{e}");
}

#[test]
fn currency_url_and_response_parse() {
    assert_eq!(
        currency_url(25.0, "usd", "php"),
        "https://api.frankfurter.dev/v1/latest?amount=25&from=USD&to=PHP"
    );
    let body = r#"{"amount":25.0,"base":"USD","rates":{"PHP":1412.5}}"#;
    assert_eq!(parse_currency_response(body, "php").unwrap(), 1412.5);

    let e = parse_currency_response(r#"{"rates":{}}"#, "php").unwrap_err().to_string();
    assert!(e.contains("no rate for 'php'"), "{e}");
    let e = parse_currency_response("not json", "php").unwrap_err().to_string();
    assert!(e.contains("malformed JSON"), "{e}");
}
