//! `ls` — list a directory's immediate entries (the Claude-Code `LS` tool,
//! completing the glob/grep/ls coding triad). Non-recursive (use `glob` for
//! trees); dirs first then files, each with kind + byte size. Jailed via
//! `ToolContext::resolve`.

use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};

const MAX_ENTRIES: usize = 500;

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "ls".into(),
        description:
            "List a directory's immediate entries (name, dir|file, size). Non-recursive — \
                      use glob for whole trees, search_files to grep contents. Defaults to the \
                      working directory."
                .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Directory to list (default: cwd)."}
            }
        }),
        toolset: "file".into(),
    }
}

pub struct LsTool;

#[async_trait]
impl ToolExecutor for LsTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let dir = match args.get("path").and_then(Value::as_str) {
            Some(p) => match ctx.resolve(p) {
                Ok(resolved) => resolved,
                Err(error) => return Ok(tool_error_json(error.to_string())),
            },
            None => ctx.cwd.clone(),
        };
        let mut rd = match tokio::fs::read_dir(&dir).await {
            Ok(rd) => rd,
            Err(error) => {
                return Ok(tool_error_json(format!(
                    "cannot list {}: {error}",
                    dir.display()
                )));
            }
        };
        let mut entries: Vec<(bool, String, u64)> = Vec::new(); // (is_dir, name, size)
        let mut truncated = false;
        while let Ok(Some(entry)) = rd.next_entry().await {
            if entries.len() >= MAX_ENTRIES {
                truncated = true;
                break;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let (is_dir, size) = match entry.metadata().await {
                Ok(m) => (m.is_dir(), m.len()),
                Err(_) => (false, 0),
            };
            entries.push((is_dir, name, size));
        }
        // Dirs first, then files; alphabetical within each.
        entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let items: Vec<_> = entries
            .iter()
            .map(|(is_dir, name, size)| {
                json!({"name": name, "kind": if *is_dir { "dir" } else { "file" }, "size": size})
            })
            .collect();
        Ok(
            json!({"path": dir.display().to_string(), "entries": items, "truncated": truncated})
                .to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::contracts::DenyAll;
    use std::sync::Arc;

    #[tokio::test]
    async fn lists_dirs_first_then_files() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::create_dir(dir.path().join("sub")).await.unwrap();
        tokio::fs::write(dir.path().join("a.txt"), "hi")
            .await
            .unwrap();
        let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
        let out = LsTool.execute(json!({}), &ctx).await.unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        let entries = v["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["kind"], "dir"); // dirs first
        assert_eq!(entries[0]["name"], "sub");
        assert_eq!(entries[1]["name"], "a.txt");
        assert_eq!(entries[1]["size"], 2);
    }

    #[tokio::test]
    async fn missing_dir_is_error_json() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
        let out = LsTool.execute(json!({"path": "nope"}), &ctx).await.unwrap();
        assert!(out.contains("cannot list"), "got: {out}");
    }
}
