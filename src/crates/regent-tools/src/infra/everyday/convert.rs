//! `convert` — everyday unit and currency conversion. Units are converted
//! offline from a fixed factor table (length, mass, temperature, volume,
//! area, speed, data); currency goes through frankfurter.dev (ECB reference
//! rates, keyless). Only the currency path touches the network, and a failed
//! call names the service — never a silent empty result.
//! (~205 lines: the unit factor table IS most of the file — splitting it
//! from the tool that reads it would hurt, not help.)

use super::http;
use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_result_json};
use serde_json::{Value, json};

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "convert".into(),
        description: "Convert a value between units (length, mass, temperature, volume, area, \
                      speed, data — offline) or between currencies by 3-letter ISO code like \
                      USD→PHP (live ECB reference rates, no key). Args: value, from, to."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "value": {"type": "number", "description": "The amount to convert."},
                "from": {"type": "string", "description": "Source unit (e.g. \"km\", \"lb\", \"C\") or currency code (e.g. \"USD\")."},
                "to": {"type": "string", "description": "Target unit or currency code."}
            },
            "required": ["value", "from", "to"]
        }),
        toolset: "everyday".into(),
    }
}

pub struct ConvertTool;

#[async_trait]
impl ToolExecutor for ConvertTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let value = args
            .get("value")
            .and_then(Value::as_f64)
            .ok_or_else(|| bad("`value` (a number) is required"))?;
        let from = unit_arg(&args, "from")?;
        let to = unit_arg(&args, "to")?;

        if let Some(result) = convert_units(value, &from, &to)? {
            return Ok(tool_result_json(result));
        }
        if is_currency_code(&from) && is_currency_code(&to) {
            return convert_currency(value, &from, &to).await;
        }
        Err(bad(format!(
            "unknown units '{from}' → '{to}' — supported: length (mm/cm/m/km/in/ft/yd/mi), \
             mass (mg/g/kg/t/oz/lb), temperature (C/F/K), volume (ml/l/gal/qt/pt/floz), \
             area (sqm/sqkm/sqft/sqmi/acre/ha), speed (kmh/mph/ms/knot), \
             data (b/kb/mb/gb/tb), or two 3-letter currency codes"
        )))
    }
}

fn unit_arg(args: &Value, key: &str) -> Result<String, RegentError> {
    args.get(key)
        .and_then(Value::as_str)
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad(format!("`{key}` (a unit or currency code) is required")))
}

/// (canonical name, category, factor to the category's base unit).
/// Base units: m, kg, l, m², m/s, byte.
const UNITS: &[(&str, &str, f64)] = &[
    ("mm", "length", 0.001),
    ("cm", "length", 0.01),
    ("m", "length", 1.0),
    ("km", "length", 1000.0),
    ("in", "length", 0.0254),
    ("ft", "length", 0.3048),
    ("yd", "length", 0.9144),
    ("mi", "length", 1609.344),
    ("mg", "mass", 0.000_001),
    ("g", "mass", 0.001),
    ("kg", "mass", 1.0),
    ("t", "mass", 1000.0),
    ("oz", "mass", 0.028_349_523_125),
    ("lb", "mass", 0.453_592_37),
    ("ml", "volume", 0.001),
    ("l", "volume", 1.0),
    ("floz", "volume", 0.029_573_53),
    ("pt", "volume", 0.473_176),
    ("qt", "volume", 0.946_353),
    ("gal", "volume", 3.785_411_784),
    ("sqm", "area", 1.0),
    ("sqkm", "area", 1_000_000.0),
    ("sqft", "area", 0.092_903_04),
    ("sqmi", "area", 2_589_988.11),
    ("acre", "area", 4_046.856_422_4),
    ("ha", "area", 10_000.0),
    ("ms", "speed", 1.0),
    ("kmh", "speed", 0.277_777_78),
    ("mph", "speed", 0.447_04),
    ("knot", "speed", 0.514_444),
    ("b", "data", 1.0),
    ("kb", "data", 1_000.0),
    ("mb", "data", 1_000_000.0),
    ("gb", "data", 1_000_000_000.0),
    ("tb", "data", 1_000_000_000_000.0),
];

fn lookup(unit: &str) -> Option<(&'static str, &'static str, f64)> {
    UNITS.iter().copied().find(|(name, _, _)| *name == unit)
}

/// `Ok(None)` = not a unit pair (caller may try currency).
fn convert_units(value: f64, from: &str, to: &str) -> Result<Option<Value>, RegentError> {
    if let Some(result) = convert_temperature(value, from, to) {
        return Ok(Some(result));
    }
    let (Some((f_name, f_cat, f_factor)), Some((t_name, t_cat, t_factor))) =
        (lookup(from), lookup(to))
    else {
        return Ok(None);
    };
    if f_cat != t_cat {
        return Err(bad(format!(
            "can't convert {f_cat} ('{f_name}') to {t_cat} ('{t_name}')"
        )));
    }
    let result = value * f_factor / t_factor;
    Ok(Some(json!({
        "value": value, "from": f_name, "to": t_name,
        "result": round6(result), "category": f_cat,
    })))
}

fn convert_temperature(value: f64, from: &str, to: &str) -> Option<Value> {
    let to_celsius = |v: f64, u: &str| match u {
        "c" | "celsius" => Some(v),
        "f" | "fahrenheit" => Some((v - 32.0) * 5.0 / 9.0),
        "k" | "kelvin" => Some(v - 273.15),
        _ => None,
    };
    let c = to_celsius(value, from)?;
    let result = match to {
        "c" | "celsius" => c,
        "f" | "fahrenheit" => c * 9.0 / 5.0 + 32.0,
        "k" | "kelvin" => c + 273.15,
        _ => return None,
    };
    Some(json!({
        "value": value, "from": from.to_uppercase(), "to": to.to_uppercase(),
        "result": round6(result), "category": "temperature",
    }))
}

fn is_currency_code(s: &str) -> bool {
    s.len() == 3 && s.chars().all(|c| c.is_ascii_alphabetic()) && lookup(s).is_none()
}

#[must_use]
pub fn currency_url(value: f64, from: &str, to: &str) -> String {
    // api.frankfurter.app 301s here since the v1 move — call the target
    // directly (verified live 2026-07-16).
    format!(
        "https://api.frankfurter.dev/v1/latest?amount={value}&from={}&to={}",
        from.to_uppercase(),
        to.to_uppercase()
    )
}

/// Pure parse of frankfurter's response — the testable half of the network path.
pub fn parse_currency_response(body: &str, to: &str) -> Result<f64, RegentError> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| bad(format!("frankfurter.dev returned malformed JSON: {e}")))?;
    v["rates"][to.to_uppercase()]
        .as_f64()
        .ok_or_else(|| bad(format!("frankfurter.dev response has no rate for '{to}'")))
}

async fn convert_currency(value: f64, from: &str, to: &str) -> Result<String, RegentError> {
    let url = currency_url(value, from, to);
    // Non-2xx here usually means an unknown currency code — say so.
    let bytes = http::fetch_ok("frankfurter.dev", &url)
        .await
        .map_err(|e| bad(format!("{e} — check the currency codes")))?;
    let body = String::from_utf8_lossy(&bytes);
    let result = parse_currency_response(&body, to)?;
    Ok(tool_result_json(json!({
        "value": value, "from": from.to_uppercase(), "to": to.to_uppercase(),
        "result": result, "category": "currency", "source": "frankfurter.dev (ECB reference rates)",
    })))
}

fn round6(v: f64) -> f64 {
    (v * 1_000_000.0).round() / 1_000_000.0
}

fn bad(message: impl Into<String>) -> RegentError {
    RegentError::Tool {
        tool: "convert".into(),
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "tests/convert.rs"]
mod tests;
