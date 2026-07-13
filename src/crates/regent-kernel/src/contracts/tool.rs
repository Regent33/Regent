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

/// Tools that observe but never mutate — safe to dispatch in parallel with
/// each other. Everything absent from this list is treated as mutating and
/// dispatched serially (see the turn loop); adding a name here is a deliberate
/// one-line review, never a default. The `regent` method-dispatch tool stays
/// out: some of its methods mutate (model.set, config.set, …).
// ponytail: central name list, not a per-definition flag — a `read_only` field
// on ToolDefinition would touch ~90 struct literals for the same behavior.
const READ_ONLY_TOOLS: &[&str] = &[
    "read_file",
    "glob",
    "search_files",
    "ls",
    "web_search",
    "web_fetch",
    "memory_search",
    "session_search",
    "session_list",
    "skills_list",
    "skill_view",
    "current_time",
    "vision_analyze",
];

/// Whether `name` is a read-only tool (parallel-safe). Unknown names —
/// including every MCP-provided tool — are mutating by default.
#[must_use]
pub fn is_read_only_tool(name: &str) -> bool {
    READ_ONLY_TOOLS.contains(&name)
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
