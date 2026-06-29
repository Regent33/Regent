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
        // A brand-new file is something the agent "made" — reveal it in the file
        // manager (best-effort, throttled). Overwrites/edits don't pop a window.
        let is_new = !tokio::fs::try_exists(&resolved).await.unwrap_or(true);
        match tokio::fs::write(&resolved, content).await {
            Ok(()) => {
                if is_new {
                    crate::infra::reveal::reveal(&resolved);
                }
                Ok(json!({
                    "path": resolved.display().to_string(),
                    "bytes_written": content.len(),
                })
                .to_string())
            }
            Err(error) => Ok(tool_error_json(format!(
                "cannot write {}: {error}",
                resolved.display()
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::contracts::DenyAll;
    use std::sync::Arc;

    fn ctx(dir: &std::path::Path) -> ToolContext {
        ToolContext::new(dir.to_path_buf(), Arc::new(DenyAll))
    }

    #[tokio::test]
    async fn write_then_read_round_trip_with_line_range() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ctx(dir.path());
        let written = WriteFileTool
            .execute(
                json!({"path": "notes/a.txt", "content": "one\ntwo\nthree"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(written.contains("bytes_written"));

        let read = ReadFileTool
            .execute(
                json!({"path": "notes/a.txt", "offset": 2, "limit": 1}),
                &ctx,
            )
            .await
            .unwrap();
        let value: Value = serde_json::from_str(&read).unwrap();
        assert_eq!(value["content"], "two");
        assert_eq!(value["truncated"], false);
    }

    #[tokio::test]
    async fn missing_file_is_error_json() {
        let dir = tempfile::tempdir().unwrap();
        let out = ReadFileTool
            .execute(json!({"path": "nope.txt"}), &ctx(dir.path()))
            .await
            .unwrap();
        assert!(out.contains("error"));
    }

    #[tokio::test]
    async fn sandboxed_write_outside_root_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let jailed = ToolContext::new_sandboxed(
            dir.path().to_path_buf(),
            dir.path().to_path_buf(),
            Arc::new(DenyAll),
        );
        let out = WriteFileTool
            .execute(json!({"path": "../escape.txt", "content": "x"}), &jailed)
            .await
            .unwrap();
        assert!(out.contains("escapes the sandbox"), "got: {out}");
        // And the file must not have been created outside the jail.
        assert!(!dir.path().parent().unwrap().join("escape.txt").exists());
    }
}
