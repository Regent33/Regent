use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use regex::Regex;
use serde_json::{Value, json};
use std::path::Path;

const DEFAULT_MAX_RESULTS: usize = 50;
const MAX_FILE_BYTES: u64 = 1_048_576; // skip files >1 MB
const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules", ".venv", "__pycache__"];

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "search_files".into(),
        description: "Search file contents under a directory with a regular \
                      expression. Returns matching lines with file and line number."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string", "description": "Rust-flavored regex."},
                "path": {"type": "string", "description": "Directory to search (default: cwd)."},
                "max_results": {"type": "integer", "description": "Cap on matches (default 50)."}
            },
            "required": ["pattern"]
        }),
        toolset: "file".into(),
    }
}

pub struct SearchFilesTool;

#[async_trait]
impl ToolExecutor for SearchFilesTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(pattern) = args.get("pattern").and_then(Value::as_str) else {
            return Ok(tool_error_json("missing required parameter: pattern"));
        };
        let regex = match Regex::new(pattern) {
            Ok(re) => re,
            Err(error) => return Ok(tool_error_json(format!("invalid regex: {error}"))),
        };
        let root = match args.get("path").and_then(Value::as_str) {
            Some(p) => match ctx.resolve(p) {
                Ok(resolved) => resolved,
                Err(error) => return Ok(tool_error_json(error.to_string())),
            },
            None => ctx.cwd.clone(),
        };
        let max_results = args
            .get("max_results")
            .and_then(Value::as_u64)
            .map_or(DEFAULT_MAX_RESULTS, |n| n as usize);

        // Directory walking is blocking I/O — keep it off the async runtime.
        let result =
            tokio::task::spawn_blocking(move || walk_and_match(&root, &regex, max_results))
                .await
                .map_err(|e| RegentError::Tool {
                    tool: "search_files".into(),
                    message: e.to_string(),
                })?;
        Ok(result)
    }
}

fn walk_and_match(root: &Path, regex: &Regex, max_results: usize) -> String {
    if !root.exists() {
        return tool_error_json(format!("path does not exist: {}", root.display()));
    }
    let mut matches = Vec::new();
    let mut truncated = false;
    let walker = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            !(entry.file_type().is_dir() && SKIP_DIRS.contains(&name.as_ref()))
        });
    'outer: for entry in walker.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        if entry
            .metadata()
            .map(|m| m.len() > MAX_FILE_BYTES)
            .unwrap_or(true)
        {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(entry.path()) else {
            continue; // binary or unreadable — skip silently
        };
        for (index, line) in content.lines().enumerate() {
            if regex.is_match(line) {
                if matches.len() >= max_results {
                    truncated = true;
                    break 'outer;
                }
                matches.push(json!({
                    "file": entry.path().display().to_string(),
                    "line_number": index + 1,
                    "line": line.trim_end(),
                }));
            }
        }
    }
    json!({"matches": matches, "truncated": truncated}).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::contracts::DenyAll;
    use std::sync::Arc;

    #[tokio::test]
    async fn finds_matches_and_skips_excluded_dirs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn alpha() {}\nfn beta() {}").unwrap();
        std::fs::create_dir_all(dir.path().join("node_modules")).unwrap();
        std::fs::write(dir.path().join("node_modules/b.rs"), "fn alpha_hidden() {}").unwrap();

        let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
        let out = SearchFilesTool
            .execute(json!({"pattern": "fn alpha"}), &ctx)
            .await
            .unwrap();
        let value: Value = serde_json::from_str(&out).unwrap();
        let matches = value["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1, "node_modules must be skipped");
        assert_eq!(matches[0]["line_number"], 1);
    }

    #[tokio::test]
    async fn invalid_regex_is_error_json() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
        let out = SearchFilesTool
            .execute(json!({"pattern": "("}), &ctx)
            .await
            .unwrap();
        assert!(out.contains("invalid regex"));
    }
}
