use crate::domain::errors::McpError;
use schemars::Schema;
use serde_json::Value;

/// Validates `value` against the tool's input `schema`. Under schemars 1.x a
/// `Schema` is a thin wrapper over a JSON value (either a bool or an object),
/// so we introspect the JSON keywords directly. We enforce only `type` and
/// `required` — the same surface this validator covered under schemars 0.8.
pub(crate) fn validate_input(schema: &Schema, value: &Value) -> Result<(), McpError> {
    // A boolean schema accepts (`true`) or rejects (`false`) everything.
    if let Some(accepts) = schema.as_bool() {
        return if accepts {
            Ok(())
        } else {
            Err(McpError::ToolExecution(
                "input rejected by schema".to_owned(),
            ))
        };
    }

    if let Some(types) = schema.get("type") {
        validate_type(types, value)?;
    }
    if let Some(Value::Array(required)) = schema.get("required") {
        let map = value
            .as_object()
            .ok_or_else(|| McpError::ToolExecution("expected object input".to_owned()))?;
        for key in required.iter().filter_map(Value::as_str) {
            if !map.contains_key(key) {
                return Err(McpError::ToolExecution(format!(
                    "missing required input: {key}"
                )));
            }
        }
    }
    Ok(())
}

/// `type` may be a single string or an array of strings (a union); the value
/// must match at least one.
fn validate_type(types: &Value, value: &Value) -> Result<(), McpError> {
    let matches = match types {
        Value::String(kind) => instance_matches(kind, value),
        Value::Array(kinds) => kinds
            .iter()
            .filter_map(Value::as_str)
            .any(|kind| instance_matches(kind, value)),
        // No recognizable type constraint — nothing to enforce.
        _ => true,
    };
    if matches {
        Ok(())
    } else {
        Err(McpError::ToolExecution(
            "input does not match schema type".to_owned(),
        ))
    }
}

fn instance_matches(kind: &str, value: &Value) -> bool {
    match kind {
        "null" => value.is_null(),
        "boolean" => value.is_boolean(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "string" => value.is_string(),
        // Unknown keyword: don't block.
        _ => true,
    }
}
