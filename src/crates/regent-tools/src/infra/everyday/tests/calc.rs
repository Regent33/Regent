use super::*;
use crate::domain::contracts::DenyAll;
use std::sync::Arc;

fn ctx() -> ToolContext {
    ToolContext::new(std::path::PathBuf::from("."), Arc::new(DenyAll))
}

async fn eval(expression: &str) -> Json {
    let out = CalcTool
        .execute(json!({ "expression": expression }), &ctx())
        .await
        .unwrap();
    serde_json::from_str(&out).unwrap()
}

#[tokio::test]
async fn evaluates_powers_modulo_and_builtin_functions() {
    let v = eval("2^10 + 17 % 5 + sqrt(9)").await;
    // 2^10 = 1024, 17 % 5 = 2, sqrt(9) = 3 -> 1029
    assert_eq!(v["result"].as_f64().unwrap(), 1029.0, "{v}");
}

#[tokio::test]
async fn precision_rounds_the_result() {
    let out = CalcTool
        .execute(json!({"expression": "10 / 3", "precision": 2}), &ctx())
        .await
        .unwrap();
    let v: Json = serde_json::from_str(&out).unwrap();
    assert_eq!(v["result"].as_f64().unwrap(), 3.33, "{v}");
}

#[tokio::test]
async fn division_is_a_calculator_not_an_integer_machine() {
    // The reason the evaluator is float-only: `10 / 3` must not be 3. The
    // previous engine did integer division and needed a literal-rewriting
    // pre-pass to hide it, which is the code this deleted.
    let v = eval("10 / 3").await;
    assert!(
        (v["result"].as_f64().unwrap() - 3.333_333_333_333_333_5).abs() < 1e-12,
        "{v}"
    );
}

#[tokio::test]
async fn scientific_notation_evaluates() {
    let v = eval("1.5e-3 * 2000").await;
    assert!((v["result"].as_f64().unwrap() - 3.0).abs() < 1e-9, "{v}");
}

#[tokio::test]
async fn the_supplied_functions_all_resolve() {
    // Every name `extra_functions` adds, exercised — a typo in one arm is
    // otherwise invisible until a model asks for it in production.
    for (expression, expected) in [
        ("sqrt(16)", 4.0),
        ("cbrt(27)", 3.0),
        ("exp(0)", 1.0),
        ("ln(1)", 0.0),
        ("log2(8)", 3.0),
        ("log10(1000)", 3.0),
        ("pow(2, 5)", 32.0),
        ("hypot(3, 4)", 5.0),
        ("trunc(3.9)", 3.0),
        ("fract(3.25)", 0.25),
    ] {
        let v = eval(expression).await;
        assert!(
            v["result"]
                .as_f64()
                .is_some_and(|n| (n - expected).abs() < 1e-9),
            "{expression} -> {v}"
        );
    }
}

#[tokio::test]
async fn the_old_math_namespace_still_evaluates() {
    // `math::sqrt(9)` is what this tool accepted on evalexpr and what a
    // long-running session's stored prompt may still be describing. fasteval's
    // parser stops at `::`, so without the strip these are a hard error.
    let v = eval("math::sqrt(9) + math::pow(2, 3)").await;
    assert_eq!(v["result"].as_f64().unwrap(), 11.0, "{v}");
}

#[tokio::test]
async fn print_is_refused_because_stdout_is_the_protocol() {
    let out = CalcTool
        .execute(json!({"expression": "print(1)"}), &ctx())
        .await
        .unwrap();
    assert!(out.contains("'print' is not available"), "got: {out}");
    assert!(!out.contains("\"result\""), "it must not evaluate: {out}");
}

#[tokio::test]
async fn comparisons_yield_one_or_zero() {
    assert_eq!(eval("2 < 3").await["result"].as_f64().unwrap(), 1.0);
    assert_eq!(eval("2 > 3").await["result"].as_f64().unwrap(), 0.0);
}

#[tokio::test]
async fn missing_expression_is_an_error() {
    let out = CalcTool.execute(json!({}), &ctx()).await.unwrap();
    assert!(out.contains("missing required parameter"), "got: {out}");
}

#[tokio::test]
async fn division_by_zero_is_rejected_as_non_finite() {
    let out = CalcTool
        .execute(json!({"expression": "1.0 / 0.0"}), &ctx())
        .await
        .unwrap();
    assert!(out.contains("non-finite"), "got: {out}");
}

#[tokio::test]
async fn malformed_expression_is_a_clear_error() {
    let out = CalcTool
        .execute(json!({"expression": "2 + * 3"}), &ctx())
        .await
        .unwrap();
    assert!(out.contains("cannot evaluate"), "got: {out}");
}
