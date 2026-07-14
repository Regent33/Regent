use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};

const MAX_READ_CHARS: usize = 64_000;

#[must_use]
pub fn read_definition() -> ToolDefinition {
    ToolDefinition {
        name: "read_file".into(),
        description: "Read a text file. Returns the content, optionally a line \
                      range via offset/limit (1-based offset)."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "offset": {"type": "integer", "description": "First line to return, 1-based."},
                "limit": {"type": "integer", "description": "Maximum number of lines."}
            },
            "required": ["path"]
        }),
        toolset: "file".into(),
    }
}

#[must_use]
pub fn write_definition() -> ToolDefinition {
    ToolDefinition {
        name: "write_file".into(),
        description: "Write text content to a file, creating parent directories \
                      as needed. Overwrites existing content."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "content": {"type": "string"}
            },
            "required": ["path", "content"]
        }),
        toolset: "file".into(),
    }
}

pub struct ReadFileTool;

#[async_trait]
impl ToolExecutor for ReadFileTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(path) = args.get("path").and_then(Value::as_str) else {
            return Ok(tool_error_json("missing required parameter: path"));
        };
        let resolved = match ctx.resolve(path) {
            Ok(resolved) => resolved,
            Err(error) => return Ok(tool_error_json(error.to_string())),
        };
        let raw = match tokio::fs::read_to_string(&resolved).await {
            Ok(text) => text,
            Err(error) => {
                return Ok(tool_error_json(format!(
                    "cannot read {}: {error}",
                    resolved.display()
                )));
            }
        };
        let offset = args
            .get("offset")
            .and_then(Value::as_u64)
            .map(|n| n.max(1) as usize);
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .map(|n| n as usize);
        let selected = match (offset, limit) {
            (None, None) => raw,
            (offset, limit) => {
                let start = offset.unwrap_or(1) - 1;
                let lines = raw.lines().skip(start);
                match limit {
                    Some(n) => lines.take(n).collect::<Vec<_>>().join("\n"),
                    None => lines.collect::<Vec<_>>().join("\n"),
                }
            }
        };
        let truncated = selected.chars().count() > MAX_READ_CHARS;
        let content: String = selected.chars().take(MAX_READ_CHARS).collect();
        Ok(json!({
            "path": resolved.display().to_string(),
            "content": content,
            "truncated": truncated,
        })
        .to_string())
    }
}

pub struct WriteFileTool;

#[async_trait]
impl ToolExecutor for WriteFileTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let (Some(path), Some(content)) = (
            args.get("path").and_then(Value::as_str),
            args.get("content").and_then(Value::as_str),
        ) else {
            return Ok(tool_error_json(
                "missing required parameters: path, content",
            ));
        };
        let resolved = match ctx.resolve(path) {
            Ok(resolved) => resolved,
            Err(error) => return Ok(tool_error_json(error.to_string())),
        };
        if let Some(parent) = resolved.parent()
            && let Err(error) = tokio::fs::create_dir_all(parent).await
        {
            return Ok(tool_error_json(format!(
                "cannot create parent directory {}: {error}",
                parent.display()
            )));
        }
        // No file-manager reveal here: a coding run creates many files and
        // popping Explorer for each was pure noise. Generated images (a rare,
        // user-facing artifact) keep their reveal in image_generation.
        match tokio::fs::write(&resolved, content).await {
            Ok(()) => Ok(json!({
                "path": resolved.display().to_string(),
                "bytes_written": content.len(),
            })
            .to_string()),
            Err(error) => Ok(tool_error_json(format!(
                "cannot write {}: {error}",
                resolved.display()
            ))),
        }
    }
}

#[cfg(test)]
#[path = "files_tests.rs"]
mod tests;
