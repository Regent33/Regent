//! Unit tests for `mcp_server` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

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
    build_server(NullTransport, catalog(), ctx(), server_card())
        .await
        .unwrap()
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
    assert!(
        v["result"].to_string().contains("hi"),
        "got: {}",
        v["result"]
    );
}
