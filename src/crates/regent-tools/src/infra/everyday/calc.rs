//! `calc` — evaluate a math expression offline via `fasteval`. Supports the
//! standard operators (`+ - * / % ^`), comparisons, and a function library
//! (`sqrt`, `abs`, `log`, `sin`, `min`, `max`, ...). No network, no shell —
//! pure expression evaluation.

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
                      comparisons (< > <= >= == !=, yielding 1 or 0). Functions: sqrt, cbrt, \
                      abs, sign, exp, ln, log2, log10, log(base, x), pow(x, y), hypot, \
                      sin, cos, tan, asin, acos, atan, sinh, cosh, tanh, floor, ceil, round, \
                      int, min, max, and the constants pi() and e(). \
                      Optional `precision` rounds a numeric result to that many decimal places."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "expression": {
                    "type": "string",
                    "description": "e.g. \"2^10 + sqrt(9)\" or \"17 % 5\""
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

        // `print()` is a fasteval built-in that writes to STDOUT. The deacon
        // speaks JSON-RPC over stdout, so evaluating it would inject bytes into
        // the protocol stream — from an expression a model wrote. Refused
        // before the parser ever sees it; nothing else in the library has a
        // side effect.
        if expression.contains("print") {
            return Ok(tool_error_json(
                "'print' is not available in calc — it writes to the protocol stream",
            ));
        }

        // `math::sqrt(9)` was the accepted spelling while this tool ran on
        // evalexpr, and fasteval's parser stops dead at `::`. Strip the
        // namespace so an expression written the old way still evaluates.
        let normalized = expression.replace("math::", "");

        let value = match fasteval::ez_eval(&normalized, &mut extra_functions) {
            Ok(value) => value,
            Err(error) => {
                return Ok(tool_error_json(format!(
                    "cannot evaluate '{expression}': {error}"
                )));
            }
        };

        if !value.is_finite() {
            return Ok(tool_error_json(format!(
                "'{expression}' evaluates to a non-finite result ({value}) — check for \
                 division by zero or an out-of-domain function call"
            )));
        }

        let result = match precision {
            Some(p) => round_to(value, p),
            None => value,
        };

        Ok(tool_result_json(json!({
            "expression": expression,
            "result": result,
        })))
    }
}

/// The names a calculator is expected to have that fasteval does not ship.
///
/// fasteval resolves its own built-ins first and only then consults this, so
/// nothing here can shadow `abs`, `log`, `min`, `sin` and friends. Returning
/// `None` for an unknown name is what produces the library's own
/// `Undefined("...")` error, which the caller reports verbatim.
fn extra_functions(name: &str, args: Vec<f64>) -> Option<f64> {
    match (name, args.as_slice()) {
        ("sqrt", [x]) => Some(x.sqrt()),
        ("cbrt", [x]) => Some(x.cbrt()),
        ("exp", [x]) => Some(x.exp()),
        ("ln", [x]) => Some(x.ln()),
        ("log2", [x]) => Some(x.log2()),
        ("log10", [x]) => Some(x.log10()),
        ("pow", [x, y]) => Some(x.powf(*y)),
        ("hypot", [x, y]) => Some(x.hypot(*y)),
        ("trunc", [x]) => Some(x.trunc()),
        ("fract", [x]) => Some(x.fract()),
        _ => None,
    }
}

fn round_to(n: f64, precision: i32) -> f64 {
    let factor = 10f64.powi(precision);
    (n * factor).round() / factor
}

#[cfg(test)]
#[path = "tests/calc.rs"]
mod tests;
