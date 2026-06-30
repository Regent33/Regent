//! `glob` — find files by path pattern (the Claude-Code complement to
//! `search_files`, which is content `grep`). Translates a glob (`**`, `*`, `?`)
//! to a regex and walks the tree matching relative paths. No new deps (reuses
//! `regex` + `walkdir`); the translator is pure + unit-tested.

use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regex::Regex;
use serde_json::{Value, json};
use std::path::Path;

use regent_kernel::{RegentError, ToolDefinition, tool_error_json};

const DEFAULT_MAX_RESULTS: usize = 200;
const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules", ".venv", "__pycache__"];

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "glob".into(),
        description: "Find files by path pattern (glob). `**` matches across directories, `*` \
                      within a path segment, `?` one char — e.g. `**/*.rs`, `src/**/test_*.py`. \
                      Returns matching file paths. Use search_files to grep file *contents*."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string", "description": "Glob, e.g. **/*.rs"},
                "path": {"type": "string", "description": "Directory to search (default: cwd)."},
                "max_results": {"type": "integer", "description": "Cap on matches (default 200)."}
            },
            "required": ["pattern"]
        }),
        toolset: "file".into(),
    }
}

pub struct GlobTool;

#[async_trait]
impl ToolExecutor for GlobTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(pattern) = args.get("pattern").and_then(Value::as_str) else {
            return Ok(tool_error_json("missing required parameter: pattern"));
        };
        let regex = match Regex::new(&glob_to_regex(pattern)) {
            Ok(re) => re,
            Err(error) => return Ok(tool_error_json(format!("invalid glob: {error}"))),
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

        let result =
            tokio::task::spawn_blocking(move || walk_and_match(&root, &regex, max_results))
                .await
                .map_err(|e| RegentError::Tool {
                    tool: "glob".into(),
                    message: e.to_string(),
                })?;
        Ok(result)
    }
}

fn walk_and_match(root: &Path, regex: &Regex, max_results: usize) -> String {
    if !root.exists() {
        return tool_error_json(format!("path does not exist: {}", root.display()));
    }
    let mut files = Vec::new();
    let mut truncated = false;
    let walker = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            !(entry.file_type().is_dir() && SKIP_DIRS.contains(&name.as_ref()))
        });
    for entry in walker.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        // Match the path relative to root, with forward slashes (cross-platform).
        let rel = entry.path().strip_prefix(root).unwrap_or(entry.path());
        let rel = rel.to_string_lossy().replace('\\', "/");
        if regex.is_match(&rel) {
            if files.len() >= max_results {
                truncated = true;
                break;
            }
            files.push(entry.path().display().to_string());
        }
    }
    json!({"files": files, "truncated": truncated}).to_string()
}

/// Translate a glob to an anchored regex. `**/` matches zero or more dirs, `**`
/// matches across dirs, `*` within a segment, `?` one non-slash char; regex
/// metacharacters are escaped.
fn glob_to_regex(glob: &str) -> String {
    let mut re = String::from("^");
    let chars: Vec<char> = glob.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '*' && chars.get(i + 1) == Some(&'*') {
            // `**/` ⇒ optional dir prefix; bare `**` ⇒ any chars.
            if chars.get(i + 2) == Some(&'/') {
                re.push_str("(?:.*/)?");
                i += 3;
            } else {
                re.push_str(".*");
                i += 2;
            }
            continue;
        }
        match c {
            '*' => re.push_str("[^/]*"),
            '?' => re.push_str("[^/]"),
            '.' | '+' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$' | '\\' => {
                re.push('\\');
                re.push(c);
            }
            other => re.push(other),
        }
        i += 1;
    }
    re.push('$');
    re
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::contracts::DenyAll;
    use std::sync::Arc;

    #[test]
    fn glob_translation_handles_star_and_globstar() {
        assert_eq!(glob_to_regex("*.rs"), "^[^/]*\\.rs$");
        assert_eq!(glob_to_regex("**/*.rs"), "^(?:.*/)?[^/]*\\.rs$");
        let re = Regex::new(&glob_to_regex("**/*.rs")).unwrap();
        assert!(re.is_match("a.rs"), "top-level matches");
        assert!(re.is_match("src/x/a.rs"), "nested matches");
        assert!(!re.is_match("a.py"));
    }

    #[tokio::test]
    async fn finds_files_by_pattern_skipping_excluded_dirs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/a.rs"), "x").unwrap();
        std::fs::write(dir.path().join("b.txt"), "x").unwrap();
        std::fs::create_dir_all(dir.path().join("node_modules")).unwrap();
        std::fs::write(dir.path().join("node_modules/c.rs"), "x").unwrap();

        let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
        let out = GlobTool
            .execute(json!({"pattern": "**/*.rs"}), &ctx)
            .await
            .unwrap();
        let value: Value = serde_json::from_str(&out).unwrap();
        let files = value["files"].as_array().unwrap();
        assert_eq!(files.len(), 1, "only src/a.rs (node_modules skipped)");
        assert!(files[0].as_str().unwrap().ends_with("a.rs"));
    }
}
