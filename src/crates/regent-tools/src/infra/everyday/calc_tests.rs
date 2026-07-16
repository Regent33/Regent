use super::*;
use crate::domain::contracts::DenyAll;
use std::sync::Arc;

fn ctx() -> ToolContext {
    ToolContext::new(std::path::PathBuf::from("."), Arc::new(DenyAll))
}

#[tokio::test]
async fn evaluates_powers_modulo_and_builtin_functions() {
    let ctx = ctx();
    let out = CalcTool
        .execute(json!({"expression": "2^10 + 17 % 5 + math::sqrt(9)"}), &ctx)
        .await
        .unwrap();
    let v: Json = serde_json::from_str(&out).unwrap();
    // 2^10 = 1024, 17 % 5 = 2, sqrt(9) = 3 -> 1029
    assert_eq!(v["result"].as_f64().unwrap(), 1029.0, "{v}");
}

#[tokio::test]
async fn precision_rounds_the_result() {
    let ctx = ctx();
    let out = CalcTool
        .execute(json!({"expression": "10 / 3", "precision": 2}), &ctx)
        .await
        .unwrap();
    let v: Json = serde_json::from_str(&out).unwrap();
    assert_eq!(v["result"].as_f64().unwrap(), 3.33, "{v}");
}

#[test]
fn float_normalize_leaves_scientific_notation_alone() {
    // Signed exponents must survive: appending ".0" to the exponent digits
    // ("1e-3" → "1e-3.0") made evalexpr reject ordinary calculator input.
    assert_eq!(float_normalize("1e-3"), "1e-3");
    assert_eq!(float_normalize("1.5e-10"), "1.5e-10");
    assert_eq!(float_normalize("2E+5"), "2E+5");
    assert_eq!(float_normalize("6.022e23 * 2"), "6.022e23 * 2.0");
    // ...while ordinary subtraction still gets promoted.
    assert_eq!(float_normalize("x-3"), "x-3.0");
    assert_eq!(float_normalize("10 / 3"), "10.0 / 3.0");
}

#[tokio::test]
async fn scientific_notation_evaluates() {
    let ctx = ctx();
    let out = CalcTool
        .execute(json!({"expression": "1.5e-3 * 2000"}), &ctx)
        .await
        .unwrap();
    let v: Json = serde_json::from_str(&out).unwrap();
    assert!((v["result"].as_f64().unwrap() - 3.0).abs() < 1e-9, "{v}");
}

#[tokio::test]
async fn missing_expression_is_an_error() {
    let ctx = ctx();
    let out = CalcTool.execute(json!({}), &ctx).await.unwrap();
    assert!(out.contains("missing required parameter"), "got: {out}");
}

#[tokio::test]
async fn division_by_zero_is_rejected_as_non_finite() {
    let ctx = ctx();
    let out = CalcTool
        .execute(json!({"expression": "1.0 / 0.0"}), &ctx)
        .await
        .unwrap();
    assert!(out.contains("non-finite"), "got: {out}");
}

#[tokio::test]
async fn malformed_expression_is_a_clear_error() {
    let ctx = ctx();
    let out = CalcTool
        .execute(json!({"expression": "2 + * 3"}), &ctx)
        .await
        .unwrap();
    assert!(out.contains("cannot evaluate"), "got: {out}");
}
