//! `send_message` — proactive outbound delivery. The agent names a target
//! (a connected channel) and the configured [`DeliverySink`] delivers it. The
//! tool never touches a platform SDK; the surface owns transport.
//! `send_file` uploads a local file the same way, guarded so only files under
//! the working dir or the artifacts area can leave the machine.

use crate::application::catalog::ToolCatalog;
use crate::domain::contracts::{DeliverySink, ToolExecutor};
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Registers `send_message`, wired to deliver through `sink`.
pub fn register_message_tool(
    catalog: &mut ToolCatalog,
    sink: Arc<dyn DeliverySink>,
) -> Result<(), RegentError> {
    let definition = send_message_definition(&sink.targets());
    catalog.register(definition, Arc::new(SendMessageTool { sink }))
}

/// Registers `send_file`, wired to upload through `sink`.
pub fn register_file_tool(
    catalog: &mut ToolCatalog,
    sink: Arc<dyn DeliverySink>,
) -> Result<(), RegentError> {
    catalog.register(send_file_definition(), Arc::new(SendFileTool { sink }))
}

fn send_file_definition() -> ToolDefinition {
    ToolDefinition {
        name: "send_file".into(),
        description: "Send a local file to the user on the connected channel (e.g. a document you \
                      generated). Only files inside your working directory or the artifacts folder \
                      can be sent. Provide an absolute or working-dir-relative path."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to the file to send."},
                "caption": {"type": "string", "description": "Optional caption."},
                "target": {"type": "string", "description": "Channel; omit for home."}
            },
            "required": ["path"]
        }),
        toolset: "delivery".into(),
    }
}

struct SendFileTool {
    sink: Arc<dyn DeliverySink>,
}

#[async_trait]
impl ToolExecutor for SendFileTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(path) = args
            .get("path")
            .and_then(Value::as_str)
            .filter(|p| !p.trim().is_empty())
        else {
            return Ok(tool_error_json("missing required parameter: path"));
        };
        let caption = args.get("caption").and_then(Value::as_str).unwrap_or("");
        let target = args.get("target").and_then(Value::as_str).unwrap_or("");
        let resolved = match resolve_sendable(path, &ctx.cwd) {
            Ok(p) => p,
            Err(e) => return Ok(tool_error_json(e)),
        };
        match self.sink.deliver_file(target, &resolved, caption).await {
            Ok(()) => {
                let to = if target.is_empty() { "home" } else { target };
                Ok(json!({"success": true, "delivered_to": to, "file": resolved.display().to_string()})
                    .to_string())
            }
            Err(error) => Ok(tool_error_json(error.to_string())),
        }
    }
}

/// Confine a send to a real file under the working dir or `<REGENT_HOME>/artifacts`,
/// canonicalizing first so `..` cannot escape, and block obviously-secret files
/// even inside those roots (prompt-injection exfiltration guard).
fn resolve_sendable(path: &str, cwd: &Path) -> Result<PathBuf, String> {
    let candidate = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        cwd.join(path)
    };
    let canon = candidate
        .canonicalize()
        .map_err(|_| format!("send_file: file not found: {path}"))?;
    if !canon.is_file() {
        return Err("send_file: not a regular file".into());
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(c) = cwd.canonicalize() {
        roots.push(c);
    }
    if let Ok(home) = std::env::var("REGENT_HOME")
        && let Ok(a) = Path::new(&home).join("artifacts").canonicalize()
    {
        roots.push(a);
    }
    if !roots.iter().any(|r| canon.starts_with(r)) {
        return Err(
            "send_file: only files under your working directory or the artifacts folder can be sent"
                .into(),
        );
    }
    let name = canon.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let blocked = name == ".env"
        || name == "state.db"
        || name.ends_with(".key")
        || name.ends_with(".pem")
        || name.ends_with(".db");
    if blocked {
        return Err(format!("send_file: '{name}' is blocked for safety"));
    }
    Ok(canon)
}

fn send_message_definition(targets: &[String]) -> ToolDefinition {
    let where_to = if targets.is_empty() {
        "the home channel".to_owned()
    } else {
        targets.join(", ")
    };
    ToolDefinition {
        name: "send_message".into(),
        description: format!(
            "Proactively deliver a message to the user on a connected channel. \
             Available targets: {where_to}. Omit 'target' for the home channel. \
             This sends to a real platform — use only when asked to notify or message someone."
        ),
        parameters: json!({
            "type": "object",
            "properties": {
                "text": {"type": "string", "description": "The message to deliver."},
                "target": {"type": "string", "description": "Channel to deliver to; omit for home."}
            },
            "required": ["text"]
        }),
        toolset: "delivery".into(),
    }
}

struct SendMessageTool {
    sink: Arc<dyn DeliverySink>,
}

#[async_trait]
impl ToolExecutor for SendMessageTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(text) = args.get("text").and_then(Value::as_str) else {
            return Ok(tool_error_json("missing required parameter: text"));
        };
        if text.trim().is_empty() {
            return Ok(tool_error_json("message text is empty"));
        }
        let target = args.get("target").and_then(Value::as_str).unwrap_or("");
        match self.sink.deliver(target, text).await {
            Ok(()) => {
                let to = if target.is_empty() { "home" } else { target };
                Ok(json!({"success": true, "delivered_to": to}).to_string())
            }
            Err(error) => Ok(tool_error_json(error.to_string())),
        }
    }
}

#[cfg(test)]
#[path = "message_tools_tests.rs"]
mod tests;
