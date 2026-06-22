use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// What the model sees for one tool — and nothing else. Executors live in
/// regent-tools; this struct is the schema side of the two-file contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema for the arguments object.
    pub parameters: Value,
    /// Named bundle this tool belongs to (exposure is per-toolset).
    pub toolset: String,
}

/// Every tool handler returns a JSON **string** (a core invariant — the
/// model always receives well-formed JSON, never a raw exception).
#[must_use]
pub fn tool_error_json(message: impl AsRef<str>) -> String {
    json!({ "error": message.as_ref() }).to_string()
}

#[must_use]
pub fn tool_result_json(data: Value) -> String {
    data.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_json_shape() {
        let s = tool_error_json("boom \"quoted\"");
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["error"], "boom \"quoted\"");
    }

    #[test]
    fn result_json_round_trip() {
        let s = tool_result_json(json!({"success": true, "count": 2}));
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["count"], 2);
    }
}
