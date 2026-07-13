//! `apply_patch` — apply a multi-file V4A patch (Claude Code / Hermes
//! `patch_parser` format): Add / Update / Delete files in one call. The pure
//! parser lives in `parser`; this module applies the ops through the path jail.
//! Update hunks are applied as anchored replaces — the hunk's context+removed
//! block must appear exactly in the file (like `file_edit`, but multi-line and
//! multi-hunk). Prefer `file_edit` for a single small edit; `apply_patch` for
//! coordinated multi-file/multi-hunk changes.

mod parser;

use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use parser::{HLine, Hunk, Op};
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "apply_patch".into(),
        description: "Apply a multi-file V4A patch: `*** Begin Patch`…`*** End Patch` with \
                      `*** Add File:` / `*** Update File:` / `*** Delete File:` sections; hunks \
                      use ` `/`-`/`+` lines and context must match the file exactly. For one \
                      small edit prefer file_edit."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "patch": {"type": "string", "description": "The full V4A patch text."}
            },
            "required": ["patch"]
        }),
        toolset: "file".into(),
    }
}

pub struct ApplyPatchTool;

#[async_trait]
impl ToolExecutor for ApplyPatchTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(patch) = args.get("patch").and_then(Value::as_str) else {
            return Ok(tool_error_json("missing required parameter: patch"));
        };
        let ops = match parser::parse(patch) {
            Ok(ops) => ops,
            Err(error) => return Ok(tool_error_json(format!("patch parse error: {error}"))),
        };
        let mut applied: Vec<String> = Vec::new();
        for op in ops {
            if let Err(error) = apply_one(&op, ctx, &mut applied).await {
                return Ok(tool_error_json(format!(
                    "{error} (after applying: {})",
                    applied.join(", ")
                )));
            }
        }
        Ok(json!({"ok": true, "applied": applied}).to_string())
    }
}

async fn apply_one(op: &Op, ctx: &ToolContext, applied: &mut Vec<String>) -> Result<(), String> {
    match op {
        Op::Add { path, content } => {
            let resolved = ctx.resolve(path).map_err(|e| e.to_string())?;
            if tokio::fs::try_exists(&resolved).await.unwrap_or(false) {
                return Err(format!("Add File {path}: already exists"));
            }
            if let Some(parent) = resolved.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            tokio::fs::write(&resolved, content)
                .await
                .map_err(|e| e.to_string())?;
            applied.push(format!("added {path}"));
        }
        Op::Delete { path } => {
            let resolved = ctx.resolve(path).map_err(|e| e.to_string())?;
            tokio::fs::remove_file(&resolved)
                .await
                .map_err(|e| format!("Delete File {path}: {e}"))?;
            applied.push(format!("deleted {path}"));
        }
        Op::Update { path, hunks } => {
            let resolved = ctx.resolve(path).map_err(|e| e.to_string())?;
            let mut content = tokio::fs::read_to_string(&resolved)
                .await
                .map_err(|e| format!("Update File {path}: {e}"))?;
            for (i, hunk) in hunks.iter().enumerate() {
                content = apply_hunk(&content, hunk)
                    .map_err(|e| format!("Update File {path} hunk {}: {e}", i + 1))?;
            }
            tokio::fs::write(&resolved, content)
                .await
                .map_err(|e| e.to_string())?;
            applied.push(format!("updated {path}"));
        }
    }
    Ok(())
}

/// Apply one hunk by anchored replace: the context+removed block must appear
/// exactly once; it is swapped for the context+added block.
fn apply_hunk(content: &str, hunk: &Hunk) -> Result<String, String> {
    let mut old_block = Vec::new();
    let mut new_block = Vec::new();
    for line in &hunk.lines {
        match line {
            HLine::Ctx(s) => {
                old_block.push(s.as_str());
                new_block.push(s.as_str());
            }
            HLine::Del(s) => old_block.push(s.as_str()),
            HLine::Add(s) => new_block.push(s.as_str()),
        }
    }
    let old = old_block.join("\n");
    let new = new_block.join("\n");
    if old.is_empty() {
        return Err("hunk has no context or removed lines to anchor on".into());
    }
    match content.matches(&old).count() {
        0 => Err("context/removed block not found in file".into()),
        1 => Ok(content.replacen(&old, &new, 1)),
        n => Err(format!("context/removed block is not unique ({n} matches)")),
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
    async fn add_update_delete_in_one_patch() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::write(dir.path().join("edit.txt"), "alpha\nold\nbeta")
            .await
            .unwrap();
        tokio::fs::write(dir.path().join("gone.txt"), "x")
            .await
            .unwrap();
        let patch = "*** Begin Patch\n\
                     *** Add File: new.txt\n\
                     +fresh\n\
                     *** Update File: edit.txt\n\
                     @@\n\
                      alpha\n\
                     -old\n\
                     +new\n\
                     *** Delete File: gone.txt\n\
                     *** End Patch";
        let out = ApplyPatchTool
            .execute(json!({"patch": patch}), &ctx(dir.path()))
            .await
            .unwrap();
        assert!(out.contains("\"ok\":true"), "got: {out}");
        assert_eq!(
            tokio::fs::read_to_string(dir.path().join("new.txt"))
                .await
                .unwrap(),
            "fresh"
        );
        assert_eq!(
            tokio::fs::read_to_string(dir.path().join("edit.txt"))
                .await
                .unwrap(),
            "alpha\nnew\nbeta"
        );
        assert!(!dir.path().join("gone.txt").exists(), "deleted");
    }

    #[tokio::test]
    async fn unmatched_hunk_errors_and_reports_progress() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::write(dir.path().join("f.txt"), "real")
            .await
            .unwrap();
        let patch = "*** Begin Patch\n\
                     *** Update File: f.txt\n\
                     @@\n\
                     -notthere\n\
                     +x\n\
                     *** End Patch";
        let out = ApplyPatchTool
            .execute(json!({"patch": patch}), &ctx(dir.path()))
            .await
            .unwrap();
        assert!(out.contains("not found"), "got: {out}");
    }
}
