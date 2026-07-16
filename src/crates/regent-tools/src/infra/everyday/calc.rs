//! `calc` — evaluate a math expression offline via `evalexpr`. Supports the
//! standard operators (`+ - * / % ^`), comparisons, and evalexpr's `math::*`
//! function library (`math::sqrt`, `math::pow`, `math::sin`, `math::log`,
//! `math::abs`, ...). No network, no shell — pure expression evaluation.

use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json, tool_result_json};
use serde_json::{Value as Json, json};

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "calc".into(),
        description: "Evaluate a math expression. Operators: + - * / % (modulo) ^ (power), \
                      comparisons (< > <= >= == !=), and evalexpr's math:: function library \
                      (math::sqrt, math::pow, math::sin, math::cos, math::log, math::abs, ...). \
                      Optional `precision` rounds a numeric result to that many decimal places."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "expression": {
                    "type": "string",
                    "description": "e.g. \"2^10 + math::sqrt(9)\" or \"17 % 5\""
                },
                "precision": {
                    "type": "integer",
                    "description": "Decimal places to round the result to (optional)."
                }
            },
            "required": ["expression"]
        }),
        toolset: "everyday".into(),
    }
}

pub struct CalcTool;

#[async_trait]
impl ToolExecutor for CalcTool {
    async fn execute(&self, args: Json, _ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(expression) = args
            .get("expression")
            .and_then(Json::as_str)
            .filter(|s| !s.trim().is_empty())
        else {
            return Ok(tool_error_json("missing required parameter: expression"));
        };
        let precision = args
            .get("precision")
            .and_then(Json::as_u64)
            .map(|n| n as i32);

        // Calculator semantics, not programming-language semantics: evalexpr
        // does integer division (10/3 == 3), so bare integer literals are
        // promoted to floats before evaluation.
        let value = match evalexpr::eval(&float_normalize(expression)) {
            Ok(value) => value,
            Err(error) => {
                return Ok(tool_error_json(format!(
                    "cannot evaluate '{expression}': {error}"
                )));
            }
        };

        // Non-numeric results (booleans, strings, tuples from comparisons or
        // chained expressions) are returned as their display form.
        let Ok(number) = value.as_number() else {
            return Ok(tool_result_json(json!({
                "expression": expression,
                "result": value.to_string(),
            })));
        };

        if !number.is_finite() {
            return Ok(tool_error_json(format!(
                "'{expression}' evaluates to a non-finite result ({number}) — check for \
                 division by zero or an out-of-domain function call"
            )));
        }

        let result = match precision {
            Some(p) => round_to(number, p),
            None => number,
        };

        Ok(tool_result_json(json!({
            "expression": expression,
            "result": result,
        })))
    }
}

/// Appends `.0` to every standalone integer literal so `/` divides like a
/// calculator. A digit run already touching `.`, an exponent, or an
/// identifier (e.g. `math::log2`) is left alone.
fn float_normalize(expr: &str) -> String {
    let bytes: Vec<char> = expr.chars().collect();
    let mut out = String::with_capacity(expr.len() + 8);
    let mut i = 0;
    let is_ident = |c: char| c.is_alphanumeric() || c == '_' || c == '.';
    // A digit run right after the sign of an exponent (`1e-3`, `2E+5`) is
    // part of that number — touching it would split the literal.
    let is_exponent_sign = |i: usize| {
        i >= 2
            && matches!(bytes[i - 1], '+' | '-')
            && matches!(bytes[i - 2], 'e' | 'E')
            && i >= 3
            && (bytes[i - 3].is_ascii_digit() || bytes[i - 3] == '.')
    };
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_digit() && (i == 0 || (!is_ident(bytes[i - 1]) && !is_exponent_sign(i))) {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            out.extend(&bytes[start..i]);
            let next = bytes.get(i).copied();
            if !next.is_some_and(is_ident) {
                out.push_str(".0");
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

fn round_to(n: f64, precision: i32) -> f64 {
    let factor = 10f64.powi(precision);
    (n * factor).round() / factor
}

#[cfg(test)]
#[path = "calc_tests.rs"]
mod tests;
