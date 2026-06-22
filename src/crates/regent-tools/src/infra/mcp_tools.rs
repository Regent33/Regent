//! MCP integration (Orchustr `or-mcp`): tools discovered from an MCP server
//! register into the catalog like any built-in — namespaced, schema-carried,
//! dispatched over the client. MCP is Regent's out-of-process plugin surface
//! (Footprint Ladder rung 5): third-party capability lands here, not in core.
//!
//! Design note: or-mcp's `NexusClientTrait` uses native async fns without
//! `Send` bounds, so the invoker future is boxed at a **concrete** client
//! site (where the compiler can prove Send) and registration itself stays
//! non-generic.

use crate::application::catalog::ToolCatalog;
use crate::domain::contracts::{ApprovalDecision, ToolExecutor};
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use futures::future::BoxFuture;
use or_mcp::{McpError, McpTool, NexusClient, NexusClientTrait, StreamableHttpTransport};
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::Value;
use std::sync::Arc;

/// Boxed call into an MCP server: `(remote_tool_name, args) → result`.
pub type McpInvoker =
    Arc<dyn Fn(String, Value) -> BoxFuture<'static, Result<Value, McpError>> + Send + Sync>;

/// Registers discovered MCP tools as `{namespace}_{tool}` in toolset
/// `mcp-{namespace}`. Collisions are rejected by the catalog (never
/// silently shadowed). Returns the registered local names.
pub fn register_mcp_tools(
    catalog: &mut ToolCatalog,
    tools: Vec<McpTool>,
    invoker: McpInvoker,
    namespace: &str,
) -> Result<Vec<String>, RegentError> {
    let mut registered = Vec::with_capacity(tools.len());
    for tool in tools {
        let parameters =
            serde_json::to_value(&tool.input_schema).map_err(|e| RegentError::Tool {
                tool: tool.name.clone(),
                message: format!("schema: {e}"),
            })?;
        let local_name = format!("{namespace}_{}", tool.name);
        catalog.register(
            ToolDefinition {
                name: local_name.clone(),
                description: tool.description.clone(),
                parameters,
                toolset: format!("mcp-{namespace}"),
            },
            Arc::new(McpToolExecutor {
                invoker: Arc::clone(&invoker),
                remote_name: tool.name,
            }),
        )?;
        registered.push(local_name);
    }
    tracing::info!(namespace, count = registered.len(), "MCP tools registered");
    Ok(registered)
}

/// The common case: discover + register everything from one HTTP MCP server.
pub async fn register_mcp_http(
    catalog: &mut ToolCatalog,
    server_url: &str,
    namespace: &str,
) -> Result<Vec<String>, RegentError> {
    let client: NexusClient<StreamableHttpTransport> =
        NexusClient::connect_http(server_url.to_owned());
    let tools = client.list_tools().await.map_err(|e| RegentError::Tool {
        tool: format!("mcp-{namespace}"),
        message: e.to_string(),
    })?;
    let invoker: McpInvoker = Arc::new(move |name, args| {
        let client = client.clone();
        Box::pin(async move { client.invoke_tool(&name, args).await })
    });
    register_mcp_tools(catalog, tools, invoker, namespace)
}

/// Defensive cleanup for MCP string args that hold URLs (notably the browser
/// `navigate` tool): models sometimes wrap the value in quotes or drop the
/// scheme colon (`"https//search…"`), which the downstream server navigates to
/// verbatim and fails with a DNS error. Trims surrounding quotes/whitespace and
/// repairs a missing scheme colon. A no-op for well-formed args.
fn sanitize_url_args(mut args: Value) -> Value {
    if let Value::Object(map) = &mut args {
        for (key, val) in map.iter_mut() {
            if key.eq_ignore_ascii_case("url")
                && let Value::String(s) = val
            {
                *s = clean_url(s);
            }
        }
    }
    args
}

fn clean_url(raw: &str) -> String {
    let s = raw
        .trim()
        .trim_matches(|c| c == '"' || c == '\'' || c == '\u{201c}' || c == '\u{201d}')
        .trim();
    if let Some(rest) = s.strip_prefix("https//") {
        return format!("https://{rest}");
    }
    if let Some(rest) = s.strip_prefix("http//") {
        return format!("http://{rest}");
    }
    s.to_owned()
}

struct McpToolExecutor {
    invoker: McpInvoker,
    remote_name: String,
}

#[async_trait]
impl ToolExecutor for McpToolExecutor {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let args = sanitize_url_args(args);
        match (self.invoker)(self.remote_name.clone(), args).await {
            Ok(result) => Ok(result.to_string()),
            // MCP failures are data for the model, not loop crashes.
            Err(error) => Ok(tool_error_json(format!("MCP tool failed: {error}"))),
        }
    }
}

/// Attach an MCP server, registering each tool by its **own name** (no namespace
/// prefix — for well-named servers like Playwright) into `toolset`. Tools whose
/// remote name satisfies `gate` are wrapped to require approval before running
/// (the privilege gate for mutating actions). Returns the registered names.
pub async fn register_mcp_http_gated(
    catalog: &mut ToolCatalog,
    server_url: &str,
    toolset: &str,
    gate: fn(&str) -> bool,
) -> Result<Vec<String>, RegentError> {
    let client: NexusClient<StreamableHttpTransport> =
        NexusClient::connect_http(server_url.to_owned());
    let tools = client.list_tools().await.map_err(|e| RegentError::Tool {
        tool: format!("mcp-{toolset}"),
        message: e.to_string(),
    })?;
    let invoker: McpInvoker = Arc::new(move |name, args| {
        let client = client.clone();
        Box::pin(async move { client.invoke_tool(&name, args).await })
    });
    let mut registered = Vec::with_capacity(tools.len());
    for tool in tools {
        let parameters =
            serde_json::to_value(&tool.input_schema).map_err(|e| RegentError::Tool {
                tool: tool.name.clone(),
                message: format!("schema: {e}"),
            })?;
        catalog.register(
            ToolDefinition {
                name: tool.name.clone(),
                description: tool.description.clone(),
                parameters,
                toolset: toolset.to_owned(),
            },
            Arc::new(GatedMcpToolExecutor {
                invoker: Arc::clone(&invoker),
                gated: gate(&tool.name),
                remote_name: tool.name.clone(),
            }),
        )?;
        registered.push(tool.name);
    }
    tracing::info!(
        toolset,
        count = registered.len(),
        "gated MCP tools registered"
    );
    Ok(registered)
}

/// Like [`McpToolExecutor`], but asks the surface for approval before running
/// when `gated` (used for mutating browser actions: click/type/submit/…).
struct GatedMcpToolExecutor {
    invoker: McpInvoker,
    remote_name: String,
    gated: bool,
}

#[async_trait]
impl ToolExecutor for GatedMcpToolExecutor {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let args = sanitize_url_args(args);
        if self.gated {
            let summary: String = format!("{}: {args}", self.remote_name)
                .chars()
                .take(200)
                .collect();
            let decision = ctx
                .approval
                .request(&self.remote_name, &summary, "browser action")
                .await;
            if decision == ApprovalDecision::Deny {
                return Ok(tool_error_json(format!(
                    "{} denied by approval policy",
                    self.remote_name
                )));
            }
        }
        match (self.invoker)(self.remote_name.clone(), args).await {
            Ok(result) => Ok(result.to_string()),
            Err(error) => Ok(tool_error_json(format!("MCP tool failed: {error}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn repairs_quoted_and_schemeless_urls() {
        // The exact shape from the field report: leading quote + dropped colon.
        let out = sanitize_url_args(json!({"url": "\"https//search.brave.com/search?q=Claude+fable+5\""}));
        assert_eq!(out["url"], "https://search.brave.com/search?q=Claude+fable+5");
    }

    #[test]
    fn leaves_well_formed_urls_and_other_args_untouched() {
        let out = sanitize_url_args(json!({"url": "https://example.com/x", "selector": "\"#id\""}));
        assert_eq!(out["url"], "https://example.com/x");
        assert_eq!(out["selector"], "\"#id\""); // only `url` is touched
    }
}
