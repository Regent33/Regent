//! `move_file` · `copy_file` · `delete_file` — file/folder mutations via
//! `std::fs` (so spaces in paths just work — no shell quoting), each **gated
//! through the surface's approval handler** (CLI prompt / Telegram `/approve`):
//! a denied or unattended call never touches the disk. Directories are handled
//! recursively. Paths resolve through the `ToolContext` (sandbox-aware when a
//! jail is configured).

use crate::domain::contracts::{ApprovalDecision, ToolExecutor};
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[must_use]
pub fn move_definition() -> ToolDefinition {
    ToolDefinition {
        name: "move_file".into(),
        description: "Move or rename a file or folder (source → destination). Handles spaces in \
                      paths. Asks for approval first. Use this to relocate a project, e.g. into the \
                      artifacts folder."
            .into(),
        parameters: schema_src_dst(),
        toolset: "file".into(),
    }
}

#[must_use]
pub fn copy_definition() -> ToolDefinition {
    ToolDefinition {
        name: "copy_file".into(),
        description: "Copy a file or folder (recursively) from source to destination. Handles \
                      spaces in paths. Asks for approval first."
            .into(),
        parameters: schema_src_dst(),
        toolset: "file".into(),
    }
}

#[must_use]
pub fn delete_definition() -> ToolDefinition {
    ToolDefinition {
        name: "delete_file".into(),
        description: "Delete a file or folder (folders are removed recursively). Destructive — \
                      asks for approval first and cannot be undone."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": { "path": {"type": "string", "description": "File or folder to delete."} },
            "required": ["path"]
        }),
        toolset: "file".into(),
    }
}

fn schema_src_dst() -> Value {
    json!({
        "type": "object",
        "properties": {
            "source": {"type": "string", "description": "Path to move/copy from."},
            "destination": {"type": "string", "description": "Path to move/copy to."}
        },
        "required": ["source", "destination"]
    })
}

pub struct MoveFileTool;
pub struct CopyFileTool;
pub struct DeleteFileTool;

#[async_trait]
impl ToolExecutor for MoveFileTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let (src, dst) = match resolve_src_dst(&args, ctx) {
            Ok(pair) => pair,
            Err(e) => return Ok(tool_error_json(e)),
        };
        if !gate(ctx, "move_file", &format!("Move {} → {}", src.display(), dst.display())).await {
            return Ok(tool_error_json("move_file denied by approval policy"));
        }
        run_blocking("move_file", move || {
            if let Some(p) = dst.parent() {
                std::fs::create_dir_all(p)?;
            }
            // rename is atomic on one volume; fall back to copy+remove across volumes.
            match std::fs::rename(&src, &dst) {
                Ok(()) => Ok(()),
                Err(_) => {
                    copy_recursive(&src, &dst)?;
                    remove_path(&src)
                }
            }
            .map(|()| json!({"success": true, "from": src.display().to_string(), "to": dst.display().to_string()}))
        })
        .await
    }
}

#[async_trait]
impl ToolExecutor for CopyFileTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let (src, dst) = match resolve_src_dst(&args, ctx) {
            Ok(pair) => pair,
            Err(e) => return Ok(tool_error_json(e)),
        };
        if !gate(ctx, "copy_file", &format!("Copy {} → {}", src.display(), dst.display())).await {
            return Ok(tool_error_json("copy_file denied by approval policy"));
        }
        run_blocking("copy_file", move || {
            copy_recursive(&src, &dst)
                .map(|()| json!({"success": true, "from": src.display().to_string(), "to": dst.display().to_string()}))
        })
        .await
    }
}

#[async_trait]
impl ToolExecutor for DeleteFileTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(path) = args.get("path").and_then(Value::as_str).filter(|p| !p.trim().is_empty())
        else {
            return Ok(tool_error_json("missing required parameter: path"));
        };
        let path = match ctx.resolve(path) {
            Ok(p) => p,
            Err(e) => return Ok(tool_error_json(e.to_string())),
        };
        if !gate(ctx, "delete_file", &format!("Delete {}", path.display())).await {
            return Ok(tool_error_json("delete_file denied by approval policy"));
        }
        run_blocking("delete_file", move || {
            remove_path(&path).map(|()| json!({"success": true, "deleted": path.display().to_string()}))
        })
        .await
    }
}

/// Resolve `source`/`destination` through the context (sandbox-aware).
fn resolve_src_dst(args: &Value, ctx: &ToolContext) -> Result<(PathBuf, PathBuf), String> {
    let src = args.get("source").and_then(Value::as_str).filter(|s| !s.trim().is_empty());
    let dst = args.get("destination").and_then(Value::as_str).filter(|s| !s.trim().is_empty());
    let (Some(src), Some(dst)) = (src, dst) else {
        return Err("missing required parameters: source, destination".into());
    };
    let src = ctx.resolve(src).map_err(|e| e.to_string())?;
    let dst = ctx.resolve(dst).map_err(|e| e.to_string())?;
    if !src.exists() {
        return Err(format!("source does not exist: {}", src.display()));
    }
    Ok((src, dst))
}

/// Request approval; non-response/`Deny` → false (never proceed by default).
async fn gate(ctx: &ToolContext, tool: &str, action: &str) -> bool {
    ctx.approval.request(tool, action, "file mutation").await == ApprovalDecision::Approve
}

/// Run a blocking fs closure off the runtime, mapping its result to JSON.
async fn run_blocking<F>(tool: &'static str, f: F) -> Result<String, RegentError>
where
    F: FnOnce() -> std::io::Result<Value> + Send + 'static,
{
    tokio::task::spawn_blocking(move || match f() {
        Ok(v) => v.to_string(),
        Err(e) => tool_error_json(format!("{tool} failed: {e}")),
    })
    .await
    .map_err(|e| RegentError::Tool { tool: tool.into(), message: e.to_string() })
}

fn remove_path(path: &Path) -> std::io::Result<()> {
    if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) }
}

/// Recursively copy a file or directory tree.
fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let to = dst.join(entry.file_name());
            if entry.path().is_dir() {
                copy_recursive(&entry.path(), &to)?;
            } else {
                std::fs::copy(entry.path(), &to)?;
            }
        }
        Ok(())
    } else {
        if let Some(p) = dst.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::copy(src, dst).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::contracts::{ApprovalHandler, DenyAll};
    use std::sync::Arc;

    struct AllowAll;
    #[async_trait]
    impl ApprovalHandler for AllowAll {
        async fn request(&self, _: &str, _: &str, _: &str) -> ApprovalDecision {
            ApprovalDecision::Approve
        }
    }

    fn ctx(dir: &Path, approval: Arc<dyn ApprovalHandler>) -> ToolContext {
        ToolContext::new(dir.to_path_buf(), approval)
    }

    #[tokio::test]
    async fn copy_then_move_then_delete_with_approval() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"hi").unwrap();
        let ctx = ctx(dir.path(), Arc::new(AllowAll));

        let copied = CopyFileTool
            .execute(json!({"source": "a.txt", "destination": "sub/b.txt"}), &ctx)
            .await
            .unwrap();
        assert!(copied.contains("\"success\":true"));
        assert!(dir.path().join("sub/b.txt").exists());

        let moved = MoveFileTool
            .execute(json!({"source": "sub/b.txt", "destination": "c.txt"}), &ctx)
            .await
            .unwrap();
        assert!(moved.contains("\"success\":true"));
        assert!(dir.path().join("c.txt").exists() && !dir.path().join("sub/b.txt").exists());

        let deleted =
            DeleteFileTool.execute(json!({"path": "c.txt"}), &ctx).await.unwrap();
        assert!(deleted.contains("\"success\":true"));
        assert!(!dir.path().join("c.txt").exists());
    }

    #[tokio::test]
    async fn denied_approval_does_not_touch_disk() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("keep.txt"), b"x").unwrap();
        let out = DeleteFileTool
            .execute(json!({"path": "keep.txt"}), &ctx(dir.path(), Arc::new(DenyAll)))
            .await
            .unwrap();
        assert!(out.contains("denied by approval"));
        assert!(dir.path().join("keep.txt").exists(), "file must survive a denial");
    }
}
