//! `file_edit` — anchored, unique string-replace edit (Claude Code's FileEdit
//! contract). Replace an exact `old_string` with `new_string`; fail if it is
//! absent or non-unique. The single biggest editing win over whole-file
//! `write_file`: the model changes one spot without rewriting (or clobbering)
//! the rest. Pure core (`apply_anchored_edit`) is unit-tested; the executor
//! does the I/O through the path jail.

use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};

#[derive(Debug, PartialEq, Eq)]
pub enum EditError {
    NotFound,
    Ambiguous { count: usize },
}

/// Pure: replace the UNIQUE occurrence of `old` in `src` with `new`. Errors if
/// `old` is absent (`NotFound`) or appears more than once (`Ambiguous`) — an
/// anchored edit must be unambiguous. Callers reject an empty `old` first.
pub fn apply_anchored_edit(src: &str, old: &str, new: &str) -> Result<String, EditError> {
    match src.matches(old).count() {
        0 => Err(EditError::NotFound),
        1 => Ok(src.replacen(old, new, 1)),
        count => Err(EditError::Ambiguous { count }),
    }
}

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "file_edit".into(),
        description: "Edit a file by replacing an exact, UNIQUE snippet. `old_string` must match \
                      the file content exactly (including whitespace/indentation) and appear \
                      exactly once; the edit fails if it is missing or matches multiple places \
                      (add surrounding lines to make it unique). Prefer this over write_file when \
                      changing part of a file."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "old_string": {"type": "string", "description": "Exact snippet to replace; must be unique in the file."},
                "new_string": {"type": "string", "description": "Replacement text."}
            },
            "required": ["path", "old_string", "new_string"]
        }),
        toolset: "file".into(),
    }
}

pub struct FileEditTool;

#[async_trait]
impl ToolExecutor for FileEditTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let (Some(path), Some(old), Some(new)) = (
            args.get("path").and_then(Value::as_str),
            args.get("old_string").and_then(Value::as_str),
            args.get("new_string").and_then(Value::as_str),
        ) else {
            return Ok(tool_error_json(
                "missing required parameters: path, old_string, new_string",
            ));
        };
        if old.is_empty() {
            return Ok(tool_error_json("old_string must not be empty"));
        }
        if old == new {
            return Ok(tool_error_json(
                "old_string and new_string are identical — nothing to do",
            ));
        }
        let resolved = match ctx.resolve(path) {
            Ok(resolved) => resolved,
            Err(error) => return Ok(tool_error_json(error.to_string())),
        };
        let src = match tokio::fs::read_to_string(&resolved).await {
            Ok(text) => text,
            Err(error) => {
                return Ok(tool_error_json(format!(
                    "cannot read {}: {error}",
                    resolved.display()
                )));
            }
        };
        let edited = match apply_anchored_edit(&src, old, new) {
            Ok(edited) => edited,
            Err(EditError::NotFound) => {
                return Ok(tool_error_json(format!(
                    "old_string not found in {}",
                    resolved.display()
                )));
            }
            Err(EditError::Ambiguous { count }) => {
                return Ok(tool_error_json(format!(
                    "old_string is not unique in {} ({count} matches) — add more surrounding context",
                    resolved.display()
                )));
            }
        };
        match tokio::fs::write(&resolved, &edited).await {
            Ok(()) => {
                Ok(json!({"path": resolved.display().to_string(), "replaced": 1}).to_string())
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

    #[test]
    fn replaces_a_unique_anchor() {
        let out = apply_anchored_edit("let x = 1;\nlet y = 2;", "y = 2", "y = 3").unwrap();
        assert_eq!(out, "let x = 1;\nlet y = 3;");
    }

    #[test]
    fn missing_anchor_is_not_found() {
        assert_eq!(
            apply_anchored_edit("abc", "z", "q"),
            Err(EditError::NotFound)
        );
    }

    #[test]
    fn non_unique_anchor_is_ambiguous() {
        assert_eq!(
            apply_anchored_edit("a a a", "a", "b"),
            Err(EditError::Ambiguous { count: 3 })
        );
    }

    fn ctx(dir: &std::path::Path) -> ToolContext {
        ToolContext::new(dir.to_path_buf(), Arc::new(DenyAll))
    }

    #[tokio::test]
    async fn executor_edits_a_file_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        tokio::fs::write(&path, "hello world").await.unwrap();
        let out = FileEditTool
            .execute(
                json!({"path": "a.txt", "old_string": "world", "new_string": "regent"}),
                &ctx(dir.path()),
            )
            .await
            .unwrap();
        assert!(out.contains("\"replaced\":1"), "got: {out}");
        assert_eq!(
            tokio::fs::read_to_string(&path).await.unwrap(),
            "hello regent"
        );
    }

    #[tokio::test]
    async fn executor_reports_ambiguous_and_leaves_file_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        tokio::fs::write(&path, "x x").await.unwrap();
        let out = FileEditTool
            .execute(
                json!({"path": "a.txt", "old_string": "x", "new_string": "y"}),
                &ctx(dir.path()),
            )
            .await
            .unwrap();
        assert!(out.contains("not unique"), "got: {out}");
        assert_eq!(
            tokio::fs::read_to_string(&path).await.unwrap(),
            "x x",
            "unchanged"
        );
    }

    #[tokio::test]
    async fn executor_rejects_empty_old_string() {
        let dir = tempfile::tempdir().unwrap();
        let out = FileEditTool
            .execute(
                json!({"path": "a.txt", "old_string": "", "new_string": "y"}),
                &ctx(dir.path()),
            )
            .await
            .unwrap();
        assert!(out.contains("must not be empty"), "got: {out}");
    }
}
