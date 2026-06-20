//! `regent mcp serve` core: expose Regent's tool catalog *as* an MCP server, so
//! Regent is a tool provider, not only a consumer (P8). Builds an or-mcp
//! `NexusServer` over a server-side stdio transport, converting each Regent
//! tool definition into an MCP tool whose handler dispatches through the
//! catalog (the same guard/approval path agents use).

use crate::application::catalog::ToolCatalog;
use crate::domain::entities::ToolContext;
use or_mcp::{
    JsonRpcMessage, McpError, McpTool, McpTransport, NexusServer, NexusServerTrait, ServerCard,
};
use regent_kernel::ToolDefinition;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines, Stdin, Stdout};
use tokio::sync::Mutex;

/// MCP protocol revision advertised on `initialize`.
const PROTOCOL_VERSION: &str = "2024-11-05";

/// Regent's MCP server identity card.
#[must_use]
pub fn server_card() -> ServerCard {
    ServerCard {
        name: "regent".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        protocol_version: PROTOCOL_VERSION.to_owned(),
    }
}

/// Server-side stdio transport: reads JSON-RPC lines from *this* process's
/// stdin and writes responses to stdout — the shape an MCP client expects when
/// it spawns `regent mcp serve`. (or-mcp's `StdioTransport` is client-only: it
/// spawns a child.)
pub struct StdioServerTransport {
    reader: Mutex<Lines<BufReader<Stdin>>>,
    writer: Mutex<Stdout>,
}

impl Default for StdioServerTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl StdioServerTransport {
    #[must_use]
    pub fn new() -> Self {
        Self {
            reader: Mutex::new(BufReader::new(tokio::io::stdin()).lines()),
            writer: Mutex::new(tokio::io::stdout()),
        }
    }
}

impl McpTransport for StdioServerTransport {
    async fn send_message(&self, msg: &JsonRpcMessage) -> Result<(), McpError> {
        let line = serde_json::to_string(msg).map_err(|e| McpError::Serialization(e.to_string()))?;
        let mut writer = self.writer.lock().await;
        writer.write_all(line.as_bytes()).await.map_err(|e| McpError::Transport(e.to_string()))?;
        writer.write_all(b"\n").await.map_err(|e| McpError::Transport(e.to_string()))?;
        writer.flush().await.map_err(|e| McpError::Transport(e.to_string()))
    }

    async fn receive_message(&self) -> Result<JsonRpcMessage, McpError> {
        let mut reader = self.reader.lock().await;
        let line = reader
            .next_line()
            .await
            .map_err(|e| McpError::Transport(e.to_string()))?
            .ok_or_else(|| McpError::Transport("stdin closed".to_owned()))?;
        serde_json::from_str(&line).map_err(|e| McpError::Serialization(e.to_string()))
    }
}

/// Converts a Regent tool definition to an MCP tool. The JSON-Schema parameters
/// deserialize straight into the MCP tool's schema type.
fn to_mcp_tool(def: &ToolDefinition) -> Result<McpTool, McpError> {
    Ok(McpTool {
        name: def.name.clone(),
        description: def.description.clone(),
        input_schema: serde_json::from_value(def.parameters.clone())
            .map_err(|e| McpError::Serialization(format!("tool '{}' schema: {e}", def.name)))?,
    })
}

/// Builds (but does not run) a server exposing every tool in `catalog`. Each
/// MCP tool call is dispatched through the catalog with `ctx` — so the
/// dangerous-command guard and approval handler still apply.
pub async fn build_server<T: McpTransport>(
    transport: T,
    catalog: Arc<ToolCatalog>,
    ctx: ToolContext,
    card: ServerCard,
) -> Result<NexusServer<T>, McpError> {
    let mut server = NexusServer::new(transport, card);
    for def in catalog.definitions() {
        let tool = to_mcp_tool(&def)?;
        let cat = Arc::clone(&catalog);
        let ctx = ctx.clone();
        let name = def.name.clone();
        server
            .register_tool_handler(tool, move |args: Value| {
                let (cat, ctx, name) = (Arc::clone(&cat), ctx.clone(), name.clone());
                async move {
                    let out = cat.dispatch(&name, &args.to_string(), &ctx).await;
                    // The catalog always returns JSON; pass it through, or wrap
                    // a non-JSON string defensively.
                    Ok(serde_json::from_str::<Value>(&out).unwrap_or_else(|_| json!({ "text": out })))
                }
            })
            .await?;
    }
    Ok(server)
}

/// Builds the server and serves it over `transport` until the stream closes.
pub async fn serve_catalog<T: McpTransport>(
    transport: T,
    catalog: Arc<ToolCatalog>,
    ctx: ToolContext,
    card: ServerCard,
) -> Result<(), McpError> {
    build_server(transport, catalog, ctx, card).await?.serve().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::contracts::{DenyAll, ToolExecutor};
    use async_trait::async_trait;
    use regent_kernel::{RegentError, tool_result_json};

    struct NullTransport;
    impl McpTransport for NullTransport {
        async fn send_message(&self, _msg: &JsonRpcMessage) -> Result<(), McpError> {
            Ok(())
        }
        async fn receive_message(&self) -> Result<JsonRpcMessage, McpError> {
            Err(McpError::Transport("null".to_owned()))
        }
    }

    struct Echo;
    #[async_trait]
    impl ToolExecutor for Echo {
        async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
            Ok(tool_result_json(json!({ "echo": args["text"] })))
        }
    }

    fn catalog() -> Arc<ToolCatalog> {
        let mut c = ToolCatalog::new();
        let def = ToolDefinition {
            name: "echo".into(),
            description: "echoes text".into(),
            parameters: json!({"type": "object", "properties": {"text": {"type": "string"}}}),
            toolset: "test".into(),
        };
        c.register(def, Arc::new(Echo)).unwrap();
        Arc::new(c)
    }

    fn ctx() -> ToolContext {
        ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll))
    }

    async fn server() -> NexusServer<NullTransport> {
        build_server(NullTransport, catalog(), ctx(), server_card()).await.unwrap()
    }

    fn handle(resp: &JsonRpcMessage) -> Value {
        serde_json::to_value(resp).unwrap()
    }

    #[tokio::test]
    async fn tools_list_advertises_the_catalog() {
        let req: JsonRpcMessage =
            serde_json::from_str(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#).unwrap();
        let resp = server().await.handle_message(req).await.unwrap().unwrap();
        let v = handle(&resp);
        assert_eq!(v["result"]["tools"][0]["name"], "echo");
    }

    #[tokio::test]
    async fn tools_call_dispatches_through_the_catalog() {
        let req: JsonRpcMessage = serde_json::from_str(
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"echo","arguments":{"text":"hi"}}}"#,
        )
        .unwrap();
        let resp = server().await.handle_message(req).await.unwrap().unwrap();
        let v = handle(&resp);
        // The dispatch result is echoed back inside the JSON-RPC result.
        assert!(v["result"].to_string().contains("hi"), "got: {}", v["result"]);
    }
}
