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
use crate::domain::contracts::ToolExecutor;
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
        let parameters = serde_json::to_value(&tool.input_schema).map_err(|e| {
            RegentError::Tool { tool: tool.name.clone(), message: format!("schema: {e}") }
        })?;
        let local_name = format!("{namespace}_{}", tool.name);
        catalog.register(
            ToolDefinition {
                name: local_name.clone(),
                description: tool.description.clone(),
                parameters,
                toolset: format!("mcp-{namespace}"),
            },
            Arc::new(McpToolExecutor { invoker: Arc::clone(&invoker), remote_name: tool.name }),
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

struct McpToolExecutor {
    invoker: McpInvoker,
    remote_name: String,
}

#[async_trait]
impl ToolExecutor for McpToolExecutor {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        match (self.invoker)(self.remote_name.clone(), args).await {
            Ok(result) => Ok(result.to_string()),
            // MCP failures are data for the model, not loop crashes.
            Err(error) => Ok(tool_error_json(format!("MCP tool failed: {error}"))),
        }
    }
}
